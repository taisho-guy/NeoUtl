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

/// gpuvideo-decoderプラグイン固有の帯域外注入経路。MediaVTable(neoutl-media-api)は
/// gpu_video型へ依存させないため、libloadingで同一dylibを個別に再オープンし
/// neoutl_gpuvideo_inject_deviceシンボルを直接呼ぶ。プラグイン未配置・シンボル
/// 未検出時は無音でスキップする（gpuvideo-decoder非導入環境を許容するため）。
pub fn inject_gpuvideo_shared_device<T>(decoders_dir: &Path, device: &T) {
    let Ok(entries) = std::fs::read_dir(decoders_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_dylib(&path) {
            continue;
        }
        let Ok(lib) = (unsafe { Library::new(&path) }) else {
            continue;
        };
        let symbol: Result<Symbol<unsafe extern "C" fn(*const T)>, _> =
            unsafe { lib.get(b"neoutl_gpuvideo_inject_device\0") };
        if let Ok(inject) = symbol {
            unsafe { inject(device as *const T) };
            eprintln!("[NeoUtl] gpu_video共有デバイス注入: {}", path.display());
        }
        std::mem::forget(lib);
    }
}
