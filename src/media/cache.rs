use super::loader;
use super::worker::DecodeWorker;
use super::{MediaKind, detect_kind};
use neoutl_media_api::{AudioBuffer, ImageSource, VideoSource};
use slint::wgpu_29::wgpu;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// UIスレッド側で生成したテクスチャのLRUキャッシュ。
/// デコードスレッドはCPUバイト列のみを返すため、UIスレッドが毎フレーム
/// create_texture + write_texture を行う。同一フレームの再描画時の再アップロードを
/// 抑制するため、変換済みテクスチャを容量付きで保持する。
struct TextureLru {
    map: HashMap<i64, wgpu::Texture>,
    order: VecDeque<i64>,
    capacity: usize,
}

impl TextureLru {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
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
        while self.order.len() > self.capacity
            && let Some(evicted) = self.order.pop_front()
        {
            self.map.remove(&evicted);
        }
    }
}

/// UIスレッド側テクスチャLRUの容量。worker側リング(worker::RING_CAPACITY)と共有し、
/// config::DECODE_RING_CAPACITYを唯一の定義元とする。

struct VideoEntry {
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    /// spawn前の未使用デコーダ。frame_at初回呼び出し時にworkerへ移譲する。
    pending_decoder: Option<Box<dyn VideoSource>>,
    worker: Option<DecodeWorker>,
    /// UIスレッド側で生成したテクスチャのキャッシュ。
    /// decoder.frame_gpuの結果をキャッシュした結果を保持する。
    texture_cache: TextureLru,
    /// 直近にframe_gpuで確定したフレーム番号。目的フレーム未準備時の代用元。
    last_index: Option<i64>,
}

struct ImageEntry {
    decoder: Box<dyn ImageSource>,
    /// 画像は単一フレーム固定のため初回アップロード結果を恒久的に再利用する。
    texture: Option<wgpu::Texture>,
}

enum PathEntry {
    Video(VideoEntry),
    Image(ImageEntry),
    Audio(Arc<AudioBuffer>),
    /// open失敗を記憶し、毎フレームの再オープン試行を防ぐ。evict()で解除可能。
    Failed(String),
}

pub struct MediaCache {
    /// マップ操作（挿入・参照）のみを保護する短命ロック。デコード本体は
    /// 各パスのDecodeWorkerが専有スレッドで行うため、ここでは待機しない。
    entries: Mutex<HashMap<PathBuf, Arc<Mutex<PathEntry>>>>,
    redraw: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

fn ext_of(path: &Path) -> Result<String, String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("拡張子なし: {}", path.display()))
}

/// 拡張子に対応する動画デコーダプラグインをid昇順で順次試行する。
/// 各プラグインのopen_videoが返すErrはハードウェア/ドライバ側の実行時制約
/// （例: Vulkan Video非対応環境でのgpuvideo-decoder）を含みうるため、
/// 1プラグインの失敗だけでは即座にopen_video全体を失敗とせず次候補へ移る。
/// 全候補が失敗した場合のみ、各プラグインの理由を連結して返す。
fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    eprintln!("[media-cache] open_video開始: {}", path.display());
    let ext = ext_of(path)?;
    let candidates = loader::find_all_by_extension(&ext);
    if candidates.is_empty() {
        return Err(format!("動画デコーダ未登録: {}", path.display()));
    }

    let mut failures: Vec<String> = Vec::new();
    for plugin in candidates {
        let Some(open_fn) = plugin.vtable.open_video else {
            continue;
        };
        match open_fn(path) {
            Ok(decoder) => {
                eprintln!(
                    "[media-cache] open_video成功: {} (plugin={})",
                    path.display(),
                    plugin.id
                );
                return Ok(decoder);
            }
            Err(err) => {
                eprintln!(
                    "[media-cache] open_videoフォールバック: {} (plugin={}) 理由={err}",
                    path.display(),
                    plugin.id
                );
                failures.push(format!("{}: {err}", plugin.id));
            }
        }
    }
    Err(format!(
        "全デコーダで開けませんでした: {} [{}]",
        path.display(),
        failures.join(" / ")
    ))
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext)
        .ok_or_else(|| format!("画像デコーダ未登録: {}", path.display()))?;
    let open_fn = plugin
        .vtable
        .open_image
        .ok_or_else(|| format!("プラグイン{}はopen_image未実装", plugin.id))?;
    open_fn(path)
}

fn decode_audio(path: &Path) -> Result<AudioBuffer, String> {
    let ext = ext_of(path)?;
    let plugin = loader::find_by_extension(&ext)
        .ok_or_else(|| format!("音声デコーダ未登録: {}", path.display()))?;
    let decode_fn = plugin
        .vtable
        .decode_audio
        .ok_or_else(|| format!("プラグイン{}はdecode_audio未実装", plugin.id))?;
    decode_fn(path)
}

impl MediaCache {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            redraw: Mutex::new(None),
        }
    }

    /// デコード完了時のUI再描画要求を登録する。preview.rsのセットアップ時に一度だけ呼ぶ。
    pub fn set_redraw_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        *self.redraw.lock().unwrap() = Some(callback);
    }

    fn redraw_handle(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.redraw
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Arc::new(|| {}))
    }

    fn entry(&self, path: &Path) -> Arc<Mutex<PathEntry>> {
        {
            let map = self.entries.lock().unwrap();
            if let Some(existing) = map.get(path) {
                return existing.clone();
            }
        }
        eprintln!("[media-cache] 新規load: {}", path.display());
        let built = match detect_kind(path) {
            None => {
                let err = format!(
                    "未対応の拡張子（対応デコーダプラグイン未検出）: {}",
                    path.display()
                );
                eprintln!("[media-cache] {err}");
                PathEntry::Failed(err)
            }
            Some(MediaKind::Video) => match open_video(path) {
                Ok(decoder) => PathEntry::Video(VideoEntry {
                    width: decoder.width(),
                    height: decoder.height(),
                    fps: decoder.fps(),
                    total_frames: decoder.total_frames(),
                    pending_decoder: Some(decoder),
                    worker: None,
                    texture_cache: TextureLru::new(super::worker::RING_CAPACITY),
                    last_index: None,
                }),
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Image) => match open_image(path) {
                Ok(decoder) => PathEntry::Image(ImageEntry {
                    decoder,
                    texture: None,
                }),
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    PathEntry::Failed(err)
                }
            },
            Some(MediaKind::Audio) => match decode_audio(path) {
                Ok(buf) => PathEntry::Audio(Arc::new(buf)),
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    PathEntry::Failed(err)
                }
            },
        };
        let arc = Arc::new(Mutex::new(built));
        self.entries
            .lock()
            .unwrap()
            .entry(path.to_path_buf())
            .or_insert(arc)
            .clone()
    }

    /// UIスレッド専用。目的フレームの準備完了を確認後、decoder.frame_gpu()を
    /// worker/cache間で共有するMutex越しに直接呼びテクスチャを取得する。
    /// これにより create_texture + write_texture もデコーダ実体もUIスレッド上で
    /// 完結し、Surface::present() との wgpu SnatchLock デッドロックを回避する。
    /// 未完成時は直前に完成した最新フレームで代用し、表示の継続性を保つ。
    pub fn frame_at(
        &self,
        path: &Path,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        let entry = self.entry(path);
        let mut guard = entry.lock().unwrap();
        match &mut *guard {
            PathEntry::Video(video) => {
                if video.worker.is_none() {
                    let decoder = video
                        .pending_decoder
                        .take()
                        .expect("workerがNoneの間pending_decoderは常時Some");
                    video.worker = Some(DecodeWorker::spawn(decoder, self.redraw_handle()));
                    video.last_index = None;
                }
                let worker = video.worker.as_ref().unwrap();
                worker.request(frame_index);

                if let Some(tex) = video.texture_cache.get(frame_index) {
                    return Ok(tex);
                }

                let decoder_handle = worker.decoder_handle();
                let target_ready = worker.frame_ready(frame_index);
                let (index, is_exact) = if target_ready {
                    (frame_index, true)
                } else if let Some(last) = video.last_index {
                    (last, false)
                } else {
                    return Err("デコード中".to_string());
                };

                let tex = {
                    let mut decoder = decoder_handle.lock().unwrap();
                    decoder.frame_gpu(index, device, queue)?
                };
                if is_exact {
                    video.texture_cache.put(frame_index, tex.clone());
                    video.last_index = Some(frame_index);
                }
                Ok(tex)
            }
            PathEntry::Image(image) => {
                if image.texture.is_none() {
                    image.texture = Some(image.decoder.texture(device, queue));
                }
                Ok(image.texture.clone().unwrap())
            }
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルに映像フレームは存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    /// ソース映像/画像のピクセル寸法を返す。open時点で確定済みの値のみ参照するため
    /// デコードスレッドの完了状況に依存しない。
    pub fn dimensions(&self, path: &Path) -> Result<(u32, u32), String> {
        let entry = self.entry(path);
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok((video.width, video.height)),
            PathEntry::Image(image) => Ok((image.decoder.width(), image.decoder.height())),
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルに映像寸法は存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    /// ソース動画のフレームレート。プロジェクトFPSとの比率換算に用いる（画像/音声は不使用）。
    pub fn source_fps(&self, path: &Path) -> Result<f64, String> {
        let entry = self.entry(path);
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok(video.fps),
            PathEntry::Image(_) => Ok(0.0),
            PathEntry::Audio(_) => Err(format!(
                "音声ファイルにFPSは存在しません: {}",
                path.display()
            )),
            PathEntry::Failed(err) => Err(err.clone()),
        }
    }

    #[allow(dead_code)]
    pub fn total_frames(&self, path: &Path) -> Result<i64, String> {
        let entry = self.entry(path);
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Video(video) => Ok(video.total_frames),
            _ => Err(format!(
                "映像フレーム総数が存在しません: {}",
                path.display()
            )),
        }
    }

    pub fn audio(&self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        let entry = self.entry(path);
        let guard = entry.lock().unwrap();
        match &*guard {
            PathEntry::Audio(buffer) => Ok(buffer.clone()),
            PathEntry::Failed(err) => Err(err.clone()),
            _ => Err(format!("音声トラックが見つかりません: {}", path.display())),
        }
    }

    pub fn evict(&self, path: &Path) {
        self.entries.lock().unwrap().remove(path);
    }
}

static GLOBAL: OnceLock<MediaCache> = OnceLock::new();

pub fn global() -> &'static MediaCache {
    GLOBAL.get_or_init(MediaCache::new)
}
