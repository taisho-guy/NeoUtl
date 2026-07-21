use crate::config::{
    DECODE_PREFETCH_FAIL_THRESHOLD, DECODE_PREFETCH_RADIUS, DECODE_RING_CAPACITY,
    DECODE_WATCHDOG_TIMEOUT_MS,
};
use neoutl_media_api::VideoSource;
use slint::wgpu_29::wgpu;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

const PREFETCH_RADIUS: i64 = DECODE_PREFETCH_RADIUS;
/// UIスレッド側テクスチャLRU(media/cache.rs::TextureLru)も同容量を共有する。
/// gstreamer-decoder等CPU系デコーダの固定テクスチャプール枚数
/// (neoutl_media_api::VIDEO_TEXTURE_POOL_CAPACITY)ともstale handle aliasing回避のため
/// 同一値を維持する必要があり、GOP長等に応じた動的変更は禁止。
pub(crate) const RING_CAPACITY: usize = DECODE_RING_CAPACITY;
const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;
const DECODE_WATCHDOG_TIMEOUT: Duration = Duration::from_millis(DECODE_WATCHDOG_TIMEOUT_MS);

/// 準備完了フレームのVRAMテクスチャ保持。wgpu::Textureは参照カウント付きハンドルであり、
/// 実体はgpuvideo-decoder側の固定プール(open()時に確保済み)に存在する。ここでの
/// clone/保持は追加VRAM確保を発生させない(ゼロコピー維持)。
struct TextureStore {
    map: HashMap<i64, wgpu::Texture>,
    order: VecDeque<i64>,
}

impl TextureStore {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn contains(&self, index: i64) -> bool {
        self.map.contains_key(&index)
    }

    fn get(&self, index: i64) -> Option<wgpu::Texture> {
        self.map.get(&index).cloned()
    }

    fn put(&mut self, index: i64, texture: wgpu::Texture) {
        if self.map.contains_key(&index) {
            return;
        }
        self.map.insert(index, texture);
        self.order.push_back(index);
        while self.order.len() > RING_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
    }
}

/// decode()の監視付き実行結果。decoderはサブスレッドへ委譲される。
/// DECODE_WATCHDOG_TIMEOUT以内に完了すれば所有権はSomeで戻る。
/// タイムアウト時はNone(所有権はサブスレッド側に永久残留)。
fn watchdog_frame_gpu(
    mut decoder: Box<dyn VideoSource>,
    frame_index: i64,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
) -> (Option<Box<dyn VideoSource>>, Result<wgpu::Texture, String>) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = decoder.frame_gpu(frame_index, &device, &queue);
        let _ = tx.send((decoder, result));
    });
    match rx.recv_timeout(DECODE_WATCHDOG_TIMEOUT) {
        Ok((decoder, result)) => (Some(decoder), result),
        Err(RecvTimeoutError::Timeout) => (
            None,
            Err(format!(
                "decode watchdog timeout (frame={frame_index}, timeout={:?})",
                DECODE_WATCHDOG_TIMEOUT
            )),
        ),
        Err(RecvTimeoutError::Disconnected) => (
            None,
            Err(format!(
                "decode watchdogスレッドとの接続が切断されました (frame={frame_index})"
            )),
        ),
    }
}

pub struct DecodeWorker {
    generation: u64,
    requested: Arc<AtomicI64>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    store: Arc<Mutex<TextureStore>>,
    last_ready_index: Arc<Mutex<Option<i64>>>,
    last_error: Arc<Mutex<Option<String>>>,

    task: Option<tokio::task::JoinHandle<()>>,
    worker_thread_id: Arc<Mutex<Option<ThreadId>>>,
}

impl DecodeWorker {
    /// prefetch()とframe_gpu()(GPU decode()呼び出しを含む)を単一の永続バックグラウンド
    /// スレッドへ集約する。UIスレッドはVRAMテクスチャストア(TextureStore)の非ブロッキング
    /// 読み取り(poll_texture)のみを行い、decoder.frame_gpu()を直接呼ばない。
    /// これによりUIスレッドはGOPデコード完了を待たず毎回即座に制御を返す。
    ///
    /// decoder.frame_gpu()の監視(DECODE_WATCHDOG_TIMEOUT超過時の分離)は
    /// このバックグラウンドスレッド内で行う(watchdog_frame_gpu)。監視対象サブスレッドが
    /// gpu-videoクレート内部の無期限待機に陥った場合、当該サブスレッドごとdecoderを
    /// 永久に手放し、on_fail経由で世代切り替えを誘発してこのワーカー自身を終了する。
    /// デコーダ実体の停止がUIスレッドの応答性へ波及しない点が従来設計との相違。
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
        let store = Arc::new(Mutex::new(TextureStore::new()));
        let last_ready_index = Arc::new(Mutex::new(None));
        let last_error = Arc::new(Mutex::new(None));
        let worker_thread_id = Arc::new(Mutex::new(None));

        let requested_t = requested.clone();
        let signal_t = signal.clone();
        let store_t = store.clone();
        let last_ready_index_t = last_ready_index.clone();
        let last_error_t = last_error.clone();
        let worker_thread_id_t = worker_thread_id.clone();
        let on_fail_t = on_fail.clone();

        let task = super::runtime::handle().spawn_blocking(move || {
            *worker_thread_id_t.lock().unwrap() = Some(std::thread::current().id());

            let mut decoder = decoder;
            let total_frames_t = decoder.total_frames();

            let mut served = NONE_SENTINEL;
            let mut direction: i64 = 1;
            let mut consecutive_fails: i64 = 0;

            /// prefetch + watchdog付きframe_gpuを1フレーム分実行する。
            /// 戻り値: 継続不能(decoder永久放棄によりワーカー終了)ならfalse。
            macro_rules! produce {
                ($index:expr) => {{
                    let index = $index;
                    let mut ok = true;
                    if let Err(e) = decoder.prefetch(index) {
                        let msg = format!("prefetch(frame={index}) failed: {e}");
                        eprintln!("[decode-worker] {msg}");
                        *last_error_t.lock().unwrap() = Some(msg.clone());
                        consecutive_fails += 1;
                        if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                            on_fail_t(msg);
                            return;
                        }
                        ok = false;
                    }
                    if ok {
                        let (returned, result) =
                            watchdog_frame_gpu(decoder, index, device.clone(), queue.clone());
                        match returned {
                            Some(d) => decoder = d,
                            None => {
                                let msg = result.err().unwrap_or_default();
                                *last_error_t.lock().unwrap() = Some(msg.clone());
                                on_fail_t(msg);
                                return;
                            }
                        }
                        match result {
                            Ok(tex) => {
                                store_t.lock().unwrap().put(index, tex);
                                *last_ready_index_t.lock().unwrap() = Some(index);
                                *last_error_t.lock().unwrap() = None;
                                consecutive_fails = 0;
                            }
                            Err(e) => {
                                let msg = format!("frame_gpu(frame={index}) failed: {e}");
                                eprintln!("[decode-worker] {msg}");
                                *last_error_t.lock().unwrap() = Some(msg.clone());
                                consecutive_fails += 1;
                                if consecutive_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                    on_fail_t(msg);
                                    return;
                                }
                                ok = false;
                            }
                        }
                    }
                    ok
                }};
            }

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
                if served != NONE_SENTINEL {
                    let delta = target - served;
                    if delta != 0 {
                        direction = if delta > 0 { 1 } else { -1 };
                    }
                }

                let already_ready = store_t.lock().unwrap().contains(target);
                if already_ready {
                    served = target;
                    consecutive_fails = 0;
                } else if produce!(target) {
                    served = target;
                    on_ready();
                }

                for offset in 1..=PREFETCH_RADIUS {
                    if requested_t.load(Ordering::Acquire) != target {
                        break;
                    }
                    let ahead = target + offset * direction;
                    if ahead < 0 || ahead >= total_frames_t {
                        break;
                    }
                    if store_t.lock().unwrap().contains(ahead) {
                        continue;
                    }
                    if produce!(ahead) {
                        on_ready();
                    }
                }
            }
        });

        Self {
            generation,
            requested,
            signal,
            store,
            last_ready_index,
            last_error,
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

    /// frame_indexのVRAMテクスチャが準備完了済みか判定する。
    pub fn is_ready(&self, frame_index: i64) -> bool {
        self.store.lock().unwrap().contains(frame_index)
    }

    /// UIスレッド専用・非ブロッキング。VRAMテクスチャストアへの読み取りのみを行い、
    /// 未準備時は即座にNoneを返す。decode()呼び出しはこの呼び出しの中では発生しない
    /// (バックグラウンドスレッドが既に完了させたテクスチャのみを参照する)。
    pub fn poll_texture(&self, frame_index: i64) -> Option<wgpu::Texture> {
        self.store.lock().unwrap().get(frame_index)
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
