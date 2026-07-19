use crate::config::{DECODE_PREFETCH_RADIUS, DECODE_RING_CAPACITY};
use neoutl_media_api::VideoSource;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const PREFETCH_RADIUS: i64 = DECODE_PREFETCH_RADIUS;
/// UIスレッド側テクスチャLRU(media/cache.rs::TextureLru)も同容量を共有する。
pub(crate) const RING_CAPACITY: usize = DECODE_RING_CAPACITY;
const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;

/// 準備完了フレーム番号集合。実データは保持しない（decoder内部キャッシュが保持する）。
struct Ring {
    set: HashSet<i64>,
    order: VecDeque<i64>,
}

impl Ring {
    fn new() -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    fn contains(&self, index: i64) -> bool {
        self.set.contains(&index)
    }

    fn mark_ready(&mut self, index: i64) {
        if !self.set.contains(&index) {
            self.order.push_back(index);
            self.set.insert(index);
            if self.order.len() > RING_CAPACITY
                && let Some(evicted) = self.order.pop_front()
            {
                self.set.remove(&evicted);
            }
        }
    }
}

pub struct DecodeWorker {
    requested: Arc<AtomicI64>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    ring: Arc<Mutex<Ring>>,
    /// UIスレッド(cache.rs::frame_at)がframe_gpuを直接呼ぶための実体アクセス経路。
    decoder: Arc<Mutex<Box<dyn VideoSource>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl DecodeWorker {
    /// device/queue を取らない。workerはprefetch(パケット読出しのみ)を行い、
    /// テクスチャ生成・アップロードは呼び出し元(UIスレッド)がdecoder_handle()経由で行う。
    /// 実行スレッドはcrate::media::runtime（worker_threads設定でサイズ確定）から借りる。
    pub fn spawn(decoder: Box<dyn VideoSource>, on_ready: Arc<dyn Fn() + Send + Sync>) -> Self {
        let requested = Arc::new(AtomicI64::new(NONE_SENTINEL));
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let ring = Arc::new(Mutex::new(Ring::new()));
        let decoder = Arc::new(Mutex::new(decoder));

        let requested_t = requested.clone();
        let signal_t = signal.clone();
        let ring_t = ring.clone();
        let decoder_t = decoder.clone();

        let task = super::runtime::handle().spawn_blocking(move || {
            let mut served = NONE_SENTINEL;
            loop {
                let target = {
                    let (lock, cvar) = &*signal_t;
                    let mut pending = lock.lock().unwrap();
                    while !*pending {
                        pending = cvar.wait(pending).unwrap();
                    }
                    *pending = false;
                    requested_t.load(Ordering::Acquire)
                };
                if target == STOP_SENTINEL {
                    return;
                }
                if target == served {
                    continue;
                }
                let already_ready = ring_t.lock().unwrap().contains(target);
                if !already_ready {
                    if let Ok(mut guard) = decoder_t.try_lock()
                        && guard.prefetch(target).is_ok()
                    {
                        ring_t.lock().unwrap().mark_ready(target);
                        served = target;
                        on_ready();
                    }
                } else {
                    served = target;
                }
                for offset in 1..=PREFETCH_RADIUS {
                    if requested_t.load(Ordering::Acquire) != target {
                        break;
                    }
                    let ahead = target + offset;
                    if ring_t.lock().unwrap().contains(ahead) {
                        continue;
                    }
                    if let Ok(mut guard) = decoder_t.try_lock()
                        && guard.prefetch(ahead).is_ok()
                    {
                        ring_t.lock().unwrap().mark_ready(ahead);
                        on_ready();
                    }
                }
            }
        });

        Self {
            requested,
            signal,
            ring,
            decoder,
            task: Some(task),
        }
    }

    pub fn request(&self, frame_index: i64) {
        self.requested.store(frame_index, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();
    }

    /// 準備完了判定のみ返す。実データ取得は行わない。
    pub fn frame_ready(&self, frame_index: i64) -> bool {
        self.ring.lock().unwrap().contains(frame_index)
    }

    /// UIスレッドがframe_gpuを直接呼ぶための実体アクセス経路。
    pub fn decoder_handle(&self) -> Arc<Mutex<Box<dyn VideoSource>>> {
        self.decoder.clone()
    }
}

impl Drop for DecodeWorker {
    fn drop(&mut self) {
        self.requested.store(STOP_SENTINEL, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();
        if let Some(task) = self.task.take() {
            let _ = super::runtime::handle().block_on(task);
        }
    }
}
