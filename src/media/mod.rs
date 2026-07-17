// src/media/mod.rs
pub mod cache;
pub mod text;
pub mod worker;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

pub fn detect_kind(path: &std::path::Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "mov" | "mkv" | "webm" | "avi" => Some(MediaKind::Video),
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "gif" | "tiff" => Some(MediaKind::Image),
        "wav" | "mp3" | "flac" | "ogg" | "m4a" => Some(MediaKind::Audio),
        _ => None,
    }
}
