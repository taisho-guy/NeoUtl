// src/media/cache.rs
use super::audio::{self, AudioBuffer};
use super::image::ImageDecoder;
use super::video::VideoDecoder;
use super::{DecodedFrame, MediaKind, detect_kind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub enum MediaHandle {
    Video(VideoDecoder),
    Image(ImageDecoder),
    Audio(Arc<AudioBuffer>),
}

pub struct MediaCache {
    entries: HashMap<PathBuf, MediaHandle>,
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
                MediaKind::Video => {
                    MediaHandle::Video(VideoDecoder::open(path).map_err(|e| e.to_string())?)
                }
                MediaKind::Image => {
                    MediaHandle::Image(ImageDecoder::open(path).map_err(|e| e.to_string())?)
                }
                MediaKind::Audio => MediaHandle::Audio(Arc::new(
                    audio::decode_full(path).map_err(|e| e.to_string())?,
                )),
            };
            self.entries.insert(path.to_path_buf(), handle);
        }
        Ok(self.entries.get_mut(path).unwrap())
    }

    pub fn frame_at(&mut self, path: &Path, frame_index: i64) -> Result<DecodedFrame, String> {
        match self.load(path)? {
            MediaHandle::Video(decoder) => decoder.frame_at(frame_index).map_err(|e| e.to_string()),
            MediaHandle::Image(decoder) => Ok(decoder.frame().clone()),
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
