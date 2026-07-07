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
}

pub struct MediaCache {
    entries: HashMap<PathBuf, MediaHandle>,
}

fn open_video(path: &Path) -> Result<Box<dyn VideoSource>, String> {
    match neoutl_media_gpuvideo::GpuVideoDecoder::open(path) {
        Ok(decoder) => Ok(Box::new(decoder)),
        Err(_) => neoutl_media_ffmpeg::FfmpegVideoDecoder::open(path)
            .map(|decoder| Box::new(decoder) as Box<dyn VideoSource>)
            .map_err(|e| e.to_string()),
    }
}

fn open_image(path: &Path) -> Result<Box<dyn ImageSource>, String> {
    neoutl_media_image::StaticImageDecoder::open(path)
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
            let kind =
                detect_kind(path).ok_or_else(|| format!("未対応の拡張子: {}", path.display()))?;
            let handle = match kind {
                MediaKind::Video => MediaHandle::Video(open_video(path)?),
                MediaKind::Image => MediaHandle::Image(open_image(path)?),
                MediaKind::Audio => {
                    MediaHandle::Audio(Arc::new(neoutl_media_symphonia::decode_full(path)?))
                }
            };
            self.entries.insert(path.to_path_buf(), handle);
        }
        Ok(self.entries.get_mut(path).unwrap())
    }

    pub fn frame_at(
        &mut self,
        path: &Path,
        frame_index: i64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<wgpu::Texture, String> {
        match self.load(path)? {
            MediaHandle::Video(decoder) => decoder.frame_texture(device, queue, frame_index),
            MediaHandle::Image(decoder) => Ok(decoder.texture(device, queue)),
            MediaHandle::Audio(_) => Err(format!(
                "音声ファイルに映像フレームは存在しません: {}",
                path.display()
            )),
        }
    }

    pub fn audio(&mut self, path: &Path) -> Result<Arc<AudioBuffer>, String> {
        match self.load(path)? {
            MediaHandle::Audio(buffer) => Ok(buffer.clone()),
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
