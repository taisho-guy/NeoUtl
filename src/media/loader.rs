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
    /// dylibロードで得たプラグインのみSome。ネイティブリンクプラグイン（gpuvideo/gstreamer）は
    /// 本体と同一コンパイル単位のためLibraryを持たない。
    _lib: Option<Library>,
}

static REGISTRY: OnceLock<Vec<MediaPlugin>> = OnceLock::new();
static GPUVIDEO_VTABLE: OnceLock<MediaVTable> = OnceLock::new();
static GSTREAMER_VTABLE: OnceLock<MediaVTable> = OnceLock::new();

pub fn load_all(decoders_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut plugins: Vec<MediaPlugin> = Vec::new();
        plugins.extend(native_plugins());

        let entries = match std::fs::read_dir(decoders_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[NeoUtl] decoders/ 読み込み失敗: {err}");
                plugins.sort_by(|a, b| a.id.cmp(&b.id));
                return plugins;
            }
        };
        let candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_dylib(p))
            .collect();

        plugins.extend(candidates.iter().filter_map(|path| match load_one(path) {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("[NeoUtl] デコーダ読み込み失敗 {}: {err}", path.display());
                None
            }
        }));

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

/// gpu-video/GStreamerに依存するデコーダを本体と同一コンパイル単位のまま登録する。
/// libloading/extern "C"境界を経由しないため、wgpu::Device等の複雑な型を跨いだ
/// ABI不一致（as_hal()のNone化）が構造的に発生しない。
fn native_plugins() -> Vec<MediaPlugin> {
    let mut plugins = Vec::new();

    let gpuvideo_vtable = GPUVIDEO_VTABLE.get_or_init(neoutl_media_gpuvideo_decoder::native_vtable);
    if let Some(plugin) = build_native_plugin(gpuvideo_vtable) {
        plugins.push(plugin);
    }

    let gstreamer_vtable =
        GSTREAMER_VTABLE.get_or_init(neoutl_media_gstreamer_decoder::native_vtable);
    if let Some(plugin) = build_native_plugin(gstreamer_vtable) {
        plugins.push(plugin);
    }

    plugins
}

fn build_native_plugin(vtable: &'static MediaVTable) -> Option<MediaPlugin> {
    let meta = (vtable.meta)();
    if meta.extensions_len == 0 {
        return None;
    }
    let extensions: Vec<String> =
        unsafe { std::slice::from_raw_parts(meta.extensions_ptr, meta.extensions_len) }
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
    Some(MediaPlugin {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        kind: meta.kind,
        extensions,
        vtable,
        _lib: None,
    })
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

/// 拡張子（小文字・ドット無し）に対応する全プラグインをid昇順で返す。
/// open_video等が先頭から順に試行し、失敗時は次候補へフォールバックする用途。
/// registry()自体がid昇順ソート済みのため、フィルタのみで順序は保たれる。
pub fn find_all_by_extension(ext: &str) -> Vec<&'static MediaPlugin> {
    registry()
        .iter()
        .filter(|p| p.extensions.iter().any(|e| e == ext))
        .collect()
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
        _lib: Some(lib),
    })
}

fn is_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}

/// Vulkanデバイスをgpuvideo-decoderへ渡す。ネイティブリンクのため素の関数呼び出しであり、
/// libloadingでの再オープンやextern "C"生ポインタ受け渡しを伴わない。
#[cfg(not(target_os = "macos"))]
pub fn inject_gpuvideo_shared_device(device: std::sync::Arc<gpu_video::VulkanDevice>) {
    neoutl_media_gpuvideo_decoder::set_shared_device(device);
    eprintln!("[NeoUtl] gpu_video共有デバイス注入（ネイティブ呼び出し）");
}
