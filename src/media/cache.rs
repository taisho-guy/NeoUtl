// src/media/cache.rs
use super::worker::DecodeWorker;
use super::{MediaKind, detect_kind};
use neoutl_media_api::{AudioBuffer, ImageSource, VideoSource};
use slint::wgpu_29::wgpu;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

struct VideoEntry {
    width: u32,
    height: u32,
    fps: f64,
    total_frames: i64,
    /// spawn前の未使用デコーダ。frame_at初回呼び出し時にworkerへ移譲する。
    pending_decoder: Option<Box<dyn VideoSource>>,
    worker: Option<DecodeWorker>,
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

fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    eprintln!("[media-cache] open_video開始: {}", path.display());
    let result = neoutl_media_gstreamer_decoder::GstDecoder::open(path)
        .map(|decoder| Box::new(decoder) as Box<dyn VideoSource>);
    match &result {
        Ok(_) => eprintln!("[media-cache] open_video成功: {}", path.display()),
        Err(e) => eprintln!("[media-cache] open_video失敗: {} 理由={e}", path.display()),
    }
    result
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    neoutl_media_image_decoder::StaticImageDecoder::open(path)
        .map(|decoder| Box::new(decoder) as Box<dyn ImageSource>)
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
                let err = format!("未対応の拡張子: {}", path.display());
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
            Some(MediaKind::Audio) => match neoutl_media_symphonia_decoder::decode_full(path) {
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

    /// 完成テクスチャの即時返却のみを行う非ブロッキング呼び出し。
    /// 目的フレーム未完成時は直前に完成した最新フレームを返す。
    /// 一度も完成していない場合のみErrを返す（初回ロード直後の一瞬に限られる）。
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
                    video.worker = Some(DecodeWorker::spawn(
                        decoder,
                        device.clone(),
                        queue.clone(),
                        self.redraw_handle(),
                    ));
                }
                let worker = video.worker.as_ref().unwrap();
                worker.request(frame_index);
                worker
                    .frame(frame_index)
                    .or_else(|| worker.latest_available())
                    .ok_or_else(|| "デコード中".to_string())
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
