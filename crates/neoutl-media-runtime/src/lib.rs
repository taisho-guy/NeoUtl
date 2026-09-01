#![recursion_limit = "256"]
rust_i18n::i18n!("../../i18n");
extern crate rust_i18n;
macro_rules! t {
    ($($args:tt)*) => {
        rust_i18n::t!($($args)*).to_string()
    };
}
pub(crate) use t;

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
