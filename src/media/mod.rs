// src/media/mod.rs
pub mod cache;
pub mod loader;
pub mod runtime;
pub mod text;
pub mod worker;

pub use neoutl_media_api::MediaKind;

/// 拡張子とMediaKindの対応はデコーダプラグイン自身が申告する（loader::MediaPlugin::extensions）。
/// ホスト側で拡張子リストを固定管理しないため、新規デコーダはdylibを配置するだけで対応拡張子が増える。
pub fn detect_kind(path: &std::path::Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    loader::find_by_extension(&ext).map(|p| p.kind)
}
