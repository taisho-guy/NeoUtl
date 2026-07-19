use crate::config::{DECODE_PREFETCH_FAIL_THRESHOLD, DECODE_PREFETCH_RADIUS, DECODE_RING_CAPACITY};
use neoutl_media_api::VideoSource;
use slint::wgpu_29::wgpu;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

const PREFETCH_RADIUS: i64 = DECODE_PREFETCH_RADIUS;
/// UIスレッド側テクスチャLRU(media/cache.rs::TextureLru)も同容量を共有する。
pub(crate) const RING_CAPACITY: usize = DECODE_RING_CAPACITY;
const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;

/// 準備完了フレーム番号集合。順序保持のためringを持つ。
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
            if self.order.len() > RING_CAPACITY {
                if let Some(evicted) = self.order.pop_front() {
                    self.set.remove(&evicted);
                }
            }
        }
    }
}

pub struct DecodeWorker {
    generation: u64,
    requested: Arc<AtomicI64>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    ring: Arc<Mutex<Ring>>,
    decoder: Arc<Mutex<Box<dyn VideoSource>>>,
    last_ready_index: Arc<Mutex<Option<i64>>>,
    last_error: Arc<Mutex<Option<String>>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,

    task: Option<tokio::task::JoinHandle<()>>,
    worker_thread_id: Arc<Mutex<Option<ThreadId>>>,
}

impl DecodeWorker {
    /// worker側でprefetchだけ完結させ、frame_gpuはUIスレッドから呼び出す。
    /// on_ready: 新規フレーム準備完了時（再描画要求）
    /// on_fail: 連続失敗が閾値を超え、自身を終了する直前に一度だけ呼ばれる（reason付き）
    pub fn spawn(
        generation: u64,
        decoder: Box<dyn VideoSource>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        on_ready: Arc<dyn Fn() + Send + Sync>,
        on_fail: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        let requested = Arc::new(AtomicI64::new(NONE_SENTINEL));
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let ring = Arc::new(Mutex::new(Ring::new()));
        let decoder = Arc::new(Mutex::new(decoder));
        let last_ready_index = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));

        let worker_thread_id = Arc::new(Mutex::new(None));

        let requested_t = requested.clone();
        let signal_t = signal.clone();
        let ring_t = ring.clone();
        let decoder_t = decoder.clone();
        let last_ready_index_t = last_ready_index.clone();
        let last_error_t = last_error.clone();
        let worker_thread_id_t = worker_thread_id.clone();

        let _device = device.clone();
        let _queue = queue.clone();

        let task = super::runtime::handle().spawn_blocking(move || {
            *worker_thread_id_t.lock().unwrap() = Some(std::thread::current().id());

            let mut served = NONE_SENTINEL;
            let mut consecutive_fails: i64 = 0;

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
                    if let Ok(mut guard) = decoder_t.try_lock() {
                        match guard.prefetch(target) {
                            Ok(()) => {
                                ring_t.lock().unwrap().mark_ready(target);
                                *last_ready_index_t.lock().unwrap() = Some(target);
                                *last_error_t.lock().unwrap() = None;

                                served = target;
                                consecutive_fails = 0;
                                on_ready();
                            }
                            Err(e) => {
                                let msg = format!("prefetch(frame={target}) failed: {e}");
                                eprintln!("[decode-worker] {msg}");
                                *last_error_t.lock().unwrap() = Some(msg.clone());
                                consecutive_fails += 1;
                                if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                    on_fail(msg);
                                    return;
                                }
                            }
                        }
                    }
                } else {
                    served = target;
                    consecutive_fails = 0;
                }

                for offset in 1..=PREFETCH_RADIUS {
                    if requested_t.load(Ordering::Acquire) != target {
                        break;
                    }
                    let ahead = target + offset;
                    if ring_t.lock().unwrap().contains(ahead) {
                        continue;
                    }

                    if let Ok(mut guard) = decoder_t.try_lock() {
                        match guard.prefetch(ahead) {
                            Ok(()) => {
                                ring_t.lock().unwrap().mark_ready(ahead);
                                *last_ready_index_t.lock().unwrap() = Some(ahead);
                                *last_error_t.lock().unwrap() = None;
                                on_ready();
                            }
                            Err(e) => {
                                let msg = format!("prefetch(frame={ahead}) failed: {e}");
                                eprintln!("[decode-worker] {msg}");
                                *last_error_t.lock().unwrap() = Some(msg.clone());
                                consecutive_fails += 1;
                                if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                    on_fail(msg);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });

        Self {
            generation,
            requested,
            signal,
            ring,
            decoder,
            last_ready_index,
            last_error,
            device,
            queue,
            task: Some(task),
            worker_thread_id,
        }
    }

    pub fn request(&self, frame_index: i64) {
        self.requested.store(frame_index, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();
    }

    /// cached_texture相当をworker内では生成せず、ringにreadyが立っているときだけUI側で frame_gpu を試す。
    pub fn frame_gpu(&self, frame_index: i64) -> Result<Option<wgpu::Texture>, String> {
        if !self.ring.lock().unwrap().contains(frame_index) {
            return Ok(None);
        }

        let mut guard = match self.decoder.try_lock() {
            Ok(g) => g,
            Err(_) => return Ok(None),
        };

        match guard.frame_gpu(frame_index, &self.device, &self.queue) {
            Ok(tex) => {
                *self.last_error.lock().unwrap() = None;
                *self.last_ready_index.lock().unwrap() = Some(frame_index);
                Ok(Some(tex))
            }
            Err(e) => {
                let msg = format!("frame_gpu(frame={frame_index}) failed: {e}");
                eprintln!("[decode-worker] {msg}");
                *self.last_error.lock().unwrap() = Some(msg.clone());
                Err(msg)
            }
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn take_last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap().take()
    }
}

impl Drop for DecodeWorker {
    fn drop(&mut self) {
        self.requested.store(STOP_SENTINEL, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        *lock.lock().unwrap() = true;
        cvar.notify_one();

        if let Some(task) = self.task.take() {
            let current = std::thread::current().id();
            let worker = *self.worker_thread_id.lock().unwrap();

            if worker == Some(current) {
                task.abort();
                return;
            }

            let _ = super::runtime::handle().block_on(task);
        }
    }
}
