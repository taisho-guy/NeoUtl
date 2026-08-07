pub mod cache;
pub mod loader;
pub mod runtime;
pub mod text;
pub mod waveform;
pub mod worker;

pub use neoutl_media_api::MediaKind;

pub fn detect_kind(path: &std::path::Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    loader::find_by_extension(&ext).map(|p| p.kind)
}
