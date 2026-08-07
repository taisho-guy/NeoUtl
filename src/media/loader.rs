use arc_swap::ArcSwap;
use libloading::{Library, Symbol};
use neoutl_media_api::{AudioBuffer, ENTRY_SYMBOL, EntryFn, MediaKind, MediaVTable};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

pub struct MediaPlugin {
    pub id: String,
    pub name: String,
    pub kind: MediaKind,
    pub extensions: Vec<String>,
    pub vtable: MediaVTable,
    _lib: Option<Library>,
}

fn registry_swap() -> &'static ArcSwap<Vec<Arc<MediaPlugin>>> {
    static SWAP: OnceLock<ArcSwap<Vec<Arc<MediaPlugin>>>> = OnceLock::new();
    SWAP.get_or_init(|| ArcSwap::new(Arc::new(Vec::new())))
}

/// MediaVTable + 保持元Libraryの所有権からMediaPluginを構築する。
/// meta().id/name/extensionsはdylib静的領域参照のため、この時点で全てownedへ複製する
/// （Library解放後もMediaPlugin単体で有効であることを保証するため）。
fn from_vtable(vtable: MediaVTable, lib: Option<Library>) -> MediaPlugin {
    let meta = (vtable.meta)();
    let extensions =
        unsafe { std::slice::from_raw_parts(meta.extensions_ptr, meta.extensions_len) }
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
    MediaPlugin {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        kind: meta.kind,
        extensions,
        vtable,
        _lib: lib,
    }
}

/// NeoUtl本体へgpu_video共有デバイス注入のため直接静的リンクされるデコーダ。
/// dlsymプラグインではないためdecoders/走査対象から外れ、ここで自己登録する。
/// ffmpeg-decoderはgpuvideo-decoder（H.264ゼロコピー専用）のCPUフォールバックとして
/// 同様に直接静的リンクする（idはfind_all_by_extensionのソート順でgpuvideo後に来るよう
/// "neoutl.media.software-ffmpeg"としている。両者のextensions重複は意図的）。
fn native_plugins() -> Vec<MediaPlugin> {
    vec![
        from_vtable(gpuvideo_native_vtable(), None),
        from_vtable(neoutl_media_ffmpeg_decoder::native_vtable(), None),
    ]
}

#[cfg(target_os = "linux")]
fn gpuvideo_native_vtable() -> MediaVTable {
    neoutl_media_gpuvideo_decoder::native_vtable()
}

#[cfg(not(target_os = "linux"))]
fn gpuvideo_native_vtable() -> MediaVTable {
    neoutl_media_gpuvideo_decoder::macos_stub::native_vtable()
}

pub fn load_all(decoders_dir: &Path) {
    let mut plugins: Vec<MediaPlugin> = native_plugins();

    match std::fs::read_dir(decoders_dir) {
        Ok(entries) => {
            let candidates: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_dylib(p))
                .collect();
            for path in candidates {
                match load_one(&path) {
                    Ok(plugin) => plugins.push(plugin),
                    Err(err) => eprintln!(
                        "{}",
                        t!(
                            "[NeoUtl] デコーダ読み込み失敗 %{arg0}: %{arg1}",
                            arg0 = format!("{}", path.display()),
                            arg1 = format!("{}", err)
                        )
                    ),
                }
            }
        }
        Err(err) => eprintln!(
            "{}",
            t!(
                "[NeoUtl] decoders/ 読み込み失敗: %{arg0}",
                arg0 = format!("{}", err)
            )
        ),
    }

    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    for plugin in &plugins {
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] デコーダ登録: %{arg0} (%{arg1})",
                arg0 = format!("{}", plugin.id),
                arg1 = format!("{}", plugin.name)
            )
        );
    }
    registry_swap().store(Arc::new(plugins.into_iter().map(Arc::new).collect()));
}

pub fn registry() -> Arc<Vec<Arc<MediaPlugin>>> {
    registry_swap().load_full()
}

/// 拡張子に一致する最初のプラグイン（id昇順）。動画はVulkanゼロコピー経路優先、
/// 非対応環境ではGStreamer等のCPU経路へ自動フォールバックする序列がid昇順に一致する。
pub fn find_by_extension(ext: &str) -> Option<Arc<MediaPlugin>> {
    registry()
        .iter()
        .find(|p| p.extensions.iter().any(|e| e == ext))
        .cloned()
}

/// 拡張子に一致する全プラグイン（id昇順）。フォールバック候補列挙用。
pub fn find_all_by_extension(ext: &str) -> Vec<Arc<MediaPlugin>> {
    registry()
        .iter()
        .filter(|p| p.extensions.iter().any(|e| e == ext))
        .cloned()
        .collect()
}

pub fn decode_audio(path: &Path) -> Result<AudioBuffer, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            t!("拡張子なし: %{arg0}", arg0 = format!("{}", path.display())).to_string()
        })?;
    let plugin = find_by_extension(&ext).ok_or_else(|| {
        t!(
            "音声デコーダ未登録: %{arg0}",
            arg0 = format!("{}", path.display())
        )
        .to_string()
    })?;
    let decode_fn = plugin.vtable.decode_audio.ok_or_else(|| {
        t!(
            "プラグイン%{arg0}はdecode_audio未実装",
            arg0 = format!("{}", plugin.id)
        )
        .to_string()
    })?;
    decode_fn(path)
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

/// main.rs::gpu_shared::init_shared_gpuが起動時に一度だけ呼ぶ。gpuvideo-decoder crate内
/// SHARED_DEVICEへ委譲し、同一VulkanDeviceをデコード経路へ共有する（Linux限定機能）。
#[cfg(target_os = "linux")]
pub fn inject_gpuvideo_shared_device(device: Arc<gpu_video::VulkanDevice>) {
    neoutl_media_gpuvideo_decoder::set_shared_device(device);
}

fn load_one(path: &Path) -> Result<MediaPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable_ptr = unsafe { entry() };
    let vtable_ref: &MediaVTable = unsafe { &*vtable_ptr };
    let vtable = MediaVTable {
        meta: vtable_ref.meta,
        open_video: vtable_ref.open_video,
        open_image: vtable_ref.open_image,
        decode_audio: vtable_ref.decode_audio,
    };
    Ok(from_vtable(vtable, Some(lib)))
}

fn is_dylib(path: &Path) -> bool {
    matches!(
        path.extension().and_then(OsStr::to_str),
        Some("so" | "dylib" | "dll")
    )
}
