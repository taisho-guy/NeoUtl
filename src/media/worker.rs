use crate::config::{
    DECODE_PREFETCH_FAIL_THRESHOLD, DECODE_PREFETCH_RADIUS, DECODE_RING_CAPACITY,
    DECODE_WATCHDOG_TIMEOUT_MS,
};
use neoutl_media_api::VideoSource;
use slint::wgpu_29::wgpu;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

const PREFETCH_RADIUS: i64 = DECODE_PREFETCH_RADIUS;
/// UIスレッド側テクスチャLRU(media/cache.rs::TextureLru)も同容量を共有する。
pub(crate) const RING_CAPACITY: usize = DECODE_RING_CAPACITY;
const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;
const DECODE_WATCHDOG_TIMEOUT: Duration = Duration::from_millis(DECODE_WATCHDOG_TIMEOUT_MS);

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

/// decoder本体の所有権受け渡しスロット。
///
/// prefetch()（バックグラウンドワーカー専用・GPU操作なし）とframe_gpu()（UIスレッド専用・
/// GPU decode()呼び出しを含む）は同一VideoSourceを排他的に操作する必要があるが、
/// MutexGuardはSendでないためスレッドを跨いで保持できない。
/// そこで所有権そのもの（Box<dyn VideoSource>）をtake/putで受け渡す方式にする。
///
/// スロットがNoneの間は「他方が使用中」であり、即座に諦める（busy = 準備未完扱い）。
/// frame_gpu()のGPU decode()呼び出しがgpu-videoクレート内部で無期限停止した場合、
/// 監視スレッドごとdecoderを永久に手放す（スロットは二度とSomeへ戻らない）。
/// これにより当該decoderは事実上死亡扱いとなり、on_fail経由で世代切り替えを誘発する。
type DecoderSlot = Arc<Mutex<Option<Box<dyn VideoSource>>>>;

pub struct DecodeWorker {
    generation: u64,
    requested: Arc<AtomicI64>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    ring: Arc<Mutex<Ring>>,
    decoder_slot: DecoderSlot,
    last_ready_index: Arc<Mutex<Option<i64>>>,
    last_error: Arc<Mutex<Option<String>>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    on_fail: Arc<dyn Fn(String) + Send + Sync>,

    task: Option<tokio::task::JoinHandle<()>>,
    worker_thread_id: Arc<Mutex<Option<ThreadId>>>,
}

impl DecodeWorker {
    /// worker側でprefetchだけ完結させ、frame_gpuはUIスレッドから呼び出す。
    /// on_ready: 新規フレーム準備完了時（再描画要求）
    /// on_fail: prefetch連続失敗、またはframe_gpuの監視タイムアウトにより
    /// 自身を終了させる直前に一度だけ呼ばれる（reason付き）
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
        let decoder_slot: DecoderSlot = Arc::new(Mutex::new(Some(decoder)));
        let last_ready_index = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));

        let worker_thread_id = Arc::new(Mutex::new(None));

        let requested_t = requested.clone();
        let signal_t = signal.clone();
        let ring_t = ring.clone();
        let decoder_slot_t = decoder_slot.clone();
        let last_ready_index_t = last_ready_index.clone();
        let last_error_t = last_error.clone();
        let worker_thread_id_t = worker_thread_id.clone();
        let on_fail_t = on_fail.clone();

        let task = super::runtime::handle().spawn_blocking(move || {
            *worker_thread_id_t.lock().unwrap() = Some(std::thread::current().id());

            let mut served = NONE_SENTINEL;
            let mut consecutive_fails: i64 = 0;

            let try_prefetch = |target: i64| -> Option<Result<(), String>> {
                let mut decoder = decoder_slot_t.lock().unwrap().take()?;
                let result = decoder.prefetch(target);
                *decoder_slot_t.lock().unwrap() = Some(decoder);
                Some(result)
            };

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
                    match try_prefetch(target) {
                        Some(Ok(())) => {
                            ring_t.lock().unwrap().mark_ready(target);
                            *last_ready_index_t.lock().unwrap() = Some(target);
                            *last_error_t.lock().unwrap() = None;

                            served = target;
                            consecutive_fails = 0;
                            on_ready();
                        }
                        Some(Err(e)) => {
                            let msg = format!("prefetch(frame={target}) failed: {e}");
                            eprintln!("[decode-worker] {msg}");
                            *last_error_t.lock().unwrap() = Some(msg.clone());
                            consecutive_fails += 1;
                            if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                on_fail_t(msg);
                                return;
                            }
                        }
                        None => {}
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

                    match try_prefetch(ahead) {
                        Some(Ok(())) => {
                            ring_t.lock().unwrap().mark_ready(ahead);
                            *last_ready_index_t.lock().unwrap() = Some(ahead);
                            *last_error_t.lock().unwrap() = None;
                            on_ready();
                        }
                        Some(Err(e)) => {
                            let msg = format!("prefetch(frame={ahead}) failed: {e}");
                            eprintln!("[decode-worker] {msg}");
                            *last_error_t.lock().unwrap() = Some(msg.clone());
                            consecutive_fails += 1;
                            if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                on_fail_t(msg);
                                return;
                            }
                        }
                        None => {}
                    }
                }
            }
        });

        Self {
            generation,
            requested,
            signal,
            ring,
            decoder_slot,
            last_ready_index,
            last_error,
            device,
            queue,
            on_fail,
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

    pub fn last_ready_index(&self) -> Option<i64> {
        *self.last_ready_index.lock().unwrap()
    }

    /// frame_indexがring上で準備完了済みか判定する。
    /// last_ready_index()はahead-prefetchループが処理する全フレームで上書きされるため、
    /// 特定フレームの準備完了判定にはこちらを用いる。
    pub fn is_ready(&self, frame_index: i64) -> bool {
        self.ring.lock().unwrap().contains(frame_index)
    }

    /// UIスレッド専用。frame_indexのテクスチャを確定させる。
    ///
    /// decoder.frame_gpu()（内部でGPU decode()を呼ぶ）は専用の監視スレッドへ所有権ごと
    /// 移譲して実行し、DECODE_WATCHDOG_TIMEOUT以内に完了しなければ回収を諦める。
    /// 監視スレッドはdecoderを保持したまま永久に残留する（gpu-videoクレート内部の
    /// 無期限wait_forに起因し、Rust側からは安全に中断できないため）。
    /// 以後decoder_slotは二度とSomeへ戻らず、当該DecodeWorkerは実質的に停止する。
    pub fn frame_gpu(&self, frame_index: i64) -> Result<Option<wgpu::Texture>, String> {
        if !self.ring.lock().unwrap().contains(frame_index) {
            return Ok(None);
        }

        let Some(decoder) = self.decoder_slot.lock().unwrap().take() else {
            return Ok(None);
        };

        let device = self.device.clone();
        let queue = self.queue.clone();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut decoder = decoder;
            let result = decoder.frame_gpu(frame_index, &device, &queue);
            let _ = tx.send((decoder, result));
        });

        match rx.recv_timeout(DECODE_WATCHDOG_TIMEOUT) {
            Ok((decoder, Ok(tex))) => {
                *self.decoder_slot.lock().unwrap() = Some(decoder);
                *self.last_error.lock().unwrap() = None;
                *self.last_ready_index.lock().unwrap() = Some(frame_index);
                Ok(Some(tex))
            }
            Ok((decoder, Err(e))) => {
                *self.decoder_slot.lock().unwrap() = Some(decoder);
                let msg = format!("frame_gpu(frame={frame_index}) failed: {e}");
                if msg.contains("prefetch") || e.contains("prefetch") {
                    return Ok(None);
                }
                eprintln!("[decode-worker] {msg}");
                *self.last_error.lock().unwrap() = Some(msg.clone());
                Err(msg)
            }
            Err(RecvTimeoutError::Timeout) => {
                let msg = format!(
                    "decode watchdog timeout (frame={frame_index}, timeout={:?}) \
                     decoderを放棄しました。次回描画時に再生成します。",
                    DECODE_WATCHDOG_TIMEOUT
                );
                eprintln!("[decode-worker] {msg}");
                *self.last_error.lock().unwrap() = Some(msg.clone());
                (self.on_fail)(msg.clone());
                Err(msg)
            }
            Err(RecvTimeoutError::Disconnected) => {
                let msg = format!(
                    "decode watchdogスレッドとの接続が切断されました (frame={frame_index})"
                );
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
