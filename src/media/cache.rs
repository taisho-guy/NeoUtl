// src/media/cache.rs
use super::{MediaKind, detect_kind};
use neoutl_media_api::{AudioBuffer, ImageSource, VideoSource};
use slint::wgpu_29::wgpu;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub enum MediaHandle {
    Video(Box<dyn VideoSource>),
    Image(Box<dyn ImageSource>),
    Audio(Arc<AudioBuffer>),
    /// open失敗を記憶し、毎フレームの再オープン試行（Pipeline生成の連打・
    /// GStreamer CRITICALログのスパム）を防ぐ。evict()で解除可能。
    Failed(String),
}

pub struct MediaCache {
    entries: HashMap<PathBuf, MediaHandle>,
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
            entries: HashMap::new(),
        }
    }

    fn load(&mut self, path: &Path) -> Result<&mut MediaHandle, String> {
        if !self.entries.contains_key(path) {
            eprintln!("[media-cache] 新規load: {}", path.display());
            let kind = match detect_kind(path) {
                Some(k) => k,
                None => {
                    let err = format!("未対応の拡張子: {}", path.display());
                    eprintln!("[media-cache] {err}");
                    self.entries
                        .insert(path.to_path_buf(), MediaHandle::Failed(err.clone()));
                    return Err(err);
                }
            };
            let result = match kind {
                MediaKind::Video => open_video(path).map(MediaHandle::Video),
                MediaKind::Image => open_image(path).map(MediaHandle::Image),
                MediaKind::Audio => neoutl_media_symphonia_decoder::decode_full(path)
                    .map(|buf| MediaHandle::Audio(Arc::new(buf))),
            };
            let handle = match result {
                Ok(handle) => handle,
                Err(err) => {
                    eprintln!("[media-cache] load失敗: {} 理由={err}", path.display());
                    self.entries
                        .insert(path.to_path_buf(), MediaHandle::Failed(err.clone()));
                    return Err(err);
                }
            };
            self.entries.insert(path.to_path_buf(), handle);
        }
        match self.entries.get_mut(path).unwrap() {
            MediaHandle::Failed(err) => Err(err.clone()),
            handle => Ok(handle),
        }
    }

    pub fn frame_at(
        &mut self,
        path: &Path,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        eprintln!(
            "[media-cache] frame_at呼び出し: {} frame_index={frame_index}",
            path.display()
        );
        let result = match self.load(path)? {
            MediaHandle::Video(decoder) => decoder.frame_texture(device, queue, frame_index),
            MediaHandle::Image(decoder) => Ok(decoder.texture(device, queue)),
            MediaHandle::Audio(_) => Err(format!(
                "音声ファイルに映像フレームは存在しません: {}",
                path.display()
            )),
            MediaHandle::Failed(err) => Err(err.clone()),
        };
        match &result {
            Ok(_) => eprintln!(
                "[media-cache] frame_at成功: {} frame_index={frame_index}",
                path.display()
            ),
            Err(e) => eprintln!(
                "[media-cache] frame_at失敗: {} frame_index={frame_index} 理由={e}",
                path.display()
            ),
        }
        result
    }

    pub fn audio(&mut self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        match self.load(path)? {
            MediaHandle::Audio(buffer) => Ok(buffer.clone()),
            MediaHandle::Failed(err) => Err(err.clone()),
            _ => Err(format!("音声トラックが見つかりません: {}", path.display())),
        }
    }

    pub fn evict(&mut self, path: &Path) {
        self.entries.remove(path);
    }
}

static GLOBAL: OnceLock<Mutex<MediaCache>> = OnceLock::new();

pub fn global() -> &'static Mutex<MediaCache> {
    GLOBAL.get_or_init(|| Mutex::new(MediaCache::new()))
}
