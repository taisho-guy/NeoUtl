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

/// ここで保持するwgpu::Textureはgpuvideo-decoder側の固定プールスロットへの
/// 参照であり、実体の生存はgpuvideo側TextureCacheの容量に依存する。
/// RING_CAPACITYがgpuvideo側容量を上回ると、gpuvideo側が既に別フレーム用に
/// 再割当て済みのスロットをこちらがまだ「有効」として保持し続け、
/// 中身がすり替わったテクスチャ(stale handle aliasing)をUIへ返しかねない。
/// 投機的先読みの窓(PREFETCH_RADIUS)より大きくは絶対に広げない安全側の
/// 上限を明示的に課す。
const SAFE_RING_CAPACITY: usize = {
    let radius_window = (PREFETCH_RADIUS as usize) * 2 + 2;
    if RING_CAPACITY < radius_window {
        RING_CAPACITY
    } else {
        radius_window
    }
};

const STOP_SENTINEL: i64 = i64::MIN + 1;
const NONE_SENTINEL: i64 = i64::MIN;
const DECODE_WATCHDOG_TIMEOUT: Duration = Duration::from_millis(DECODE_WATCHDOG_TIMEOUT_MS);

/// 準備完了フレームのVRAMテクスチャ保持。wgpu::Textureは参照カウント付きハンドルであり、
/// 実体はgpuvideo-decoder側の固定プール(open()時に確保済み)に存在する。ここでの
/// clone/保持は追加VRAM確保を発生させない(ゼロコピー維持)。
///
/// get/putいずれの参照でも該当indexを最新扱いへ昇格させる真のLRUとして動作する
/// (旧実装はputでの新規挿入時のみ順序を更新し、既存hitやgetでは順序を
/// 一切更新しない単純FIFOだったため、直近に読まれた頻出フレームが
/// 未使用の古いフレームより先にevictされ得た)。
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

    fn touch(&mut self, index: i64) {
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
    }

    fn get(&mut self, index: i64) -> Option<wgpu::Texture> {
        let tex = self.map.get(&index).cloned();
        if tex.is_some() {
            self.touch(index);
        }
        tex
    }

    fn put(&mut self, index: i64, texture: wgpu::Texture) {
        if self.map.contains_key(&index) {
            self.touch(index);
            return;
        }
        self.map.insert(index, texture);
        self.order.push_back(index);
        while self.order.len() > SAFE_RING_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.map.remove(&evicted);
            }
        }
    }
}

/// バックグラウンド専有スレッドへの1件のリクエスト。
enum DecodeRequest {
    /// prefetchのみ実行する(投機的先読み窓の事前構築用。GPU decode()は伴わない)。
    PrefetchOnly(i64),
    /// prefetch + frame_gpu(GPU decode()を伴う)を実行し、確定テクスチャを返す。
    Full(i64),
}

enum DecodeResponse {
    PrefetchDone(i64, Result<(), String>),
    FrameDone(i64, Result<wgpu::Texture, String>),
}

/// decoder1個につき専有される永続バックグラウンドスレッド。
///
/// 旧実装はframe_gpu()呼び出し1回ごとにthread::spawnしていたため、再生中は
/// 毎フレームOSスレッドが生成・破棄され続けていた(ログ上でframe_gpu呼び出し毎に
/// ThreadIdが変わっていた現象の直接原因)。本実装ではDecodeThreadHandle::spawn()時に
/// 1回だけスレッドを起動し、decoder本体の所有権もそのスレッドへ完全に移す。
/// 以後この1本のスレッドがprefetch/frame_gpu双方を順に処理し続ける。
///
/// watchdogタイムアウト時、この専有スレッドはgpu-video内部の無期限待機に
/// 取り残され回収不能となる(ハードウェアデコーダのブロッキング呼び出しである以上、
/// 安全に中断する手段がないため構造的に不可避)。この場合でも新規スレッドは
/// 一切追加生成されず、当該decoderインスタンス用のスレッドが1本残留するのみに
/// 留める。またこのスレッドの内部ループが完了/終了した際に必ずログを出すため、
/// リークが発生したかどうかは常に事後確認可能(可観測)にする。
struct DecodeThreadHandle {
    req_tx: mpsc::Sender<DecodeRequest>,
    resp_rx: mpsc::Receiver<DecodeResponse>,
    hung: bool,
}

impl DecodeThreadHandle {
    fn spawn(
        mut decoder: Box<dyn VideoSource>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<DecodeRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<DecodeResponse>();

        thread::spawn(move || {
            while let Ok(req) = req_rx.recv() {
                match req {
                    DecodeRequest::PrefetchOnly(index) => {
                        let result = decoder.prefetch(index);
                        if resp_tx
                            .send(DecodeResponse::PrefetchDone(index, result))
                            .is_err()
                        {
                            break;
                        }
                    }
                    DecodeRequest::Full(index) => {
                        let result = decoder
                            .prefetch(index)
                            .and_then(|_| decoder.frame_gpu(index, &device, &queue));
                        if resp_tx
                            .send(DecodeResponse::FrameDone(index, result))
                            .is_err()
                        {
                            eprintln!(
                                "[decode-worker] frame={} decode完了もwatchdogは既に諦めていた(遅延完了) thread={:?}",
                                index,
                                thread::current().id()
                            );
                        }
                    }
                }
            }
            eprintln!(
                "[decode-worker] decode thread終了 thread={:?}",
                thread::current().id()
            );
        });

        Self {
            req_tx,
            resp_rx,
            hung: false,
        }
    }

    fn prefetch_only(&mut self, frame_index: i64) -> Result<(), String> {
        if self.hung {
            return Err(format!(
                "decoderはhung状態のためprefetch不可 (frame={frame_index})"
            ));
        }
        if self
            .req_tx
            .send(DecodeRequest::PrefetchOnly(frame_index))
            .is_err()
        {
            self.hung = true;
            return Err(format!("decode thread消失 (frame={frame_index})"));
        }
        match self.resp_rx.recv_timeout(DECODE_WATCHDOG_TIMEOUT) {
            Ok(DecodeResponse::PrefetchDone(got, result)) if got == frame_index => result,
            Ok(_) => {
                self.hung = true;
                Err(format!("decode thread応答不一致 (frame={frame_index})"))
            }
            Err(RecvTimeoutError::Timeout) => {
                self.hung = true;
                Err(format!(
                    "prefetch watchdog timeout (frame={frame_index}, timeout={:?})",
                    DECODE_WATCHDOG_TIMEOUT
                ))
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.hung = true;
                Err(format!("decode threadとの接続が切断 (frame={frame_index})"))
            }
        }
    }

    /// watchdog付きでprefetch+frame_gpuを1回実行する。タイムアウト時はこのハンドル自体を
    /// 「hung」として以後使用不能にする(内部スレッドは回収せず残留させる。
    /// 安全な強制終了手段がgpu-video側に存在しないため)。
    fn frame_gpu_watched(&mut self, frame_index: i64) -> Result<wgpu::Texture, String> {
        if self.hung {
            return Err(format!(
                "decoderは既にwatchdogタイムアウトでhung状態 (frame={frame_index})"
            ));
        }
        if self.req_tx.send(DecodeRequest::Full(frame_index)).is_err() {
            self.hung = true;
            return Err(format!("decode thread消失 (frame={frame_index})"));
        }
        match self.resp_rx.recv_timeout(DECODE_WATCHDOG_TIMEOUT) {
            Ok(DecodeResponse::FrameDone(got, result)) if got == frame_index => result,
            Ok(_) => {
                self.hung = true;
                Err(format!(
                    "decode thread応答不一致 expected={frame_index} (プロトコル破壊、以後使用不可)"
                ))
            }
            Err(RecvTimeoutError::Timeout) => {
                self.hung = true;
                Err(format!(
                    "decode watchdog timeout (frame={frame_index}, timeout={:?})",
                    DECODE_WATCHDOG_TIMEOUT
                ))
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.hung = true;
                Err(format!(
                    "decode watchdogスレッドとの接続が切断されました (frame={frame_index})"
                ))
            }
        }
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
    /// decoder.frame_gpu()の監視(DECODE_WATCHDOG_TIMEOUT超過時の分離)はdecoder本体を
    /// 専有するDecodeThreadHandleに対して行う。当該スレッドがgpu-videoクレート内部の
    /// 無期限待機に陥った場合、decoderごと手放し、on_fail経由で世代切り替えを誘発して
    /// このワーカー自身を終了する。タイムアウトのたびに新規スレッドを増やすことはない
    /// (1 decoderにつき最大1スレッド)。
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

            let total_frames_t = decoder.total_frames();
            let mut decode_thread = DecodeThreadHandle::spawn(decoder, device, queue);

            let mut served = NONE_SENTINEL;
            let mut direction: i64 = 1;
            /// 実際にUIから要求された「target」フレームの失敗のみを数える。
            /// 投機的先読み(ahead)の失敗はここへ加算しない。動画境界付近での
            /// 正常なEOF等により、本来正常に再生できるはずのワーカーが
            /// 誤ってon_fail経由で強制終了することを防ぐ。
            let mut consecutive_target_fails: i64 = 0;

            macro_rules! produce {
                ($index:expr, $critical:expr) => {{
                    let index = $index;
                    let critical: bool = $critical;
                    let result = decode_thread.frame_gpu_watched(index);
                    let mut ok = true;
                    match result {
                        Ok(tex) => {
                            store_t.lock().unwrap().put(index, tex);
                            *last_ready_index_t.lock().unwrap() = Some(index);
                            *last_error_t.lock().unwrap() = None;
                            if critical {
                                consecutive_target_fails = 0;
                            }
                        }
                        Err(e) => {
                            let msg = format!("decode(frame={index}) failed: {e}");
                            eprintln!("[decode-worker] {msg}");
                            *last_error_t.lock().unwrap() = Some(msg.clone());
                            if decode_thread.hung {
                                on_fail_t(msg);
                                return;
                            }
                            if critical {
                                consecutive_target_fails += 1;
                                if consecutive_target_fails > DECODE_PREFETCH_FAIL_THRESHOLD {
                                    on_fail_t(msg);
                                    return;
                                }
                            }
                            ok = false;
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
                    direction = if target > served { 1 } else { -1 };
                }

                let already_ready = store_t.lock().unwrap().contains(target);
                if already_ready {
                    served = target;
                    consecutive_target_fails = 0;
                } else if produce!(target, true) {
                    served = target;
                    on_ready();
                }

                let mut farthest_ahead: Option<i64> = None;
                for offset in 1..=PREFETCH_RADIUS {
                    let ahead = target + offset * direction;
                    if ahead < 0 || ahead >= total_frames_t {
                        break;
                    }
                    if !store_t.lock().unwrap().contains(ahead) {
                        farthest_ahead = Some(ahead);
                    }
                }
                if let Some(far) = farthest_ahead {
                    if requested_t.load(Ordering::Acquire) == target {
                        if let Err(e) = decode_thread.prefetch_only(far) {
                            eprintln!(
                                "[decode-worker] speculative prefetch(frame={far}) failed: {e}"
                            );
                            if decode_thread.hung {
                                on_fail_t(e);
                                return;
                            }
                        }
                    }
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
                    if produce!(ahead, false) {
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

            super::runtime::handle().spawn(async move {
                let _ = task.await;
            });
        }
    }
}
