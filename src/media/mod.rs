// src/media/mod.rs
pub mod audio;
pub mod cache;
pub mod image;
pub mod text;
pub mod video;

#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
