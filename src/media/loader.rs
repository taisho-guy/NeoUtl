// src/media/loader.rs
use libloading::{Library, Symbol};
use neoutl_media_api::{ENTRY_SYMBOL, EntryFn, MediaKind, MediaVTable};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct MediaPlugin {
    pub id: String,
    pub name: String,
    pub kind: MediaKind,
    pub extensions: Vec<String>,
    pub vtable: &'static MediaVTable,
    _lib: Library,
}

static REGISTRY: OnceLock<Vec<MediaPlugin>> = OnceLock::new();

pub fn load_all(decoders_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let entries = match std::fs::read_dir(decoders_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[NeoUtl] decoders/ 読み込み失敗: {err}");
                return Vec::new();
            }
        };
        let candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_dylib(p))
            .collect();

        let mut plugins: Vec<MediaPlugin> = candidates
            .iter()
            .filter_map(|path| match load_one(path) {
                Ok(p) => Some(p),
                Err(err) => {
                    eprintln!("[NeoUtl] デコーダ読み込み失敗 {}: {err}", path.display());
                    None
                }
            })
            .collect();

        plugins.sort_by(|a, b| a.id.cmp(&b.id));
        for plugin in &plugins {
            eprintln!(
                "[NeoUtl] デコーダ登録: {} ({}, 拡張子={:?})",
                plugin.name, plugin.id, plugin.extensions
            );
        }
        plugins
    });
}

pub fn registry() -> &'static [MediaPlugin] {
    REGISTRY.get().map(Vec::as_slice).unwrap_or(&[])
}

/// 拡張子（小文字・ドット無し）に対応する最初のプラグインを返す。
/// 複数プラグインが同一拡張子を宣言する場合はid昇順で先着したものが採用される。
pub fn find_by_extension(ext: &str) -> Option<&'static MediaPlugin> {
    registry()
        .iter()
        .find(|p| p.extensions.iter().any(|e| e == ext))
}

pub fn default_decoders_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("decoders");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/decoders");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("decoders")
}

fn load_one(path: &Path) -> Result<MediaPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static MediaVTable = unsafe { &*entry() };
    let meta = (vtable.meta)();
    let extensions: Vec<String> =
        unsafe { std::slice::from_raw_parts(meta.extensions_ptr, meta.extensions_len) }
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
    Ok(MediaPlugin {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        kind: meta.kind,
        extensions,
        vtable,
        _lib: lib,
    })
}

fn is_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}
