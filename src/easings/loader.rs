use crate::easings::registry::{CurveRegistry, default_presets_path};
use libloading::{Library, Symbol};
use neoutl_easing_api::{ENTRY_SYMBOL, EasingEngineMeta, EasingEngineVTable, EntryFn, KeyframeC};
use std::sync::Mutex;
use std::{
    ffi::{CStr, OsStr},
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct EasingPlugin {
    pub id: String,
    pub name: String,
    pub vtable: &'static EasingEngineVTable,
    _lib: Option<Library>,
}

impl EasingPlugin {
    pub fn evaluate(&self, keyframes: &[(i32, f32, Vec<u8>)], frame: i32, fallback: f32) -> f32 {
        let mut keyframes_c = Vec::with_capacity(keyframes.len());
        for (f, v, payload) in keyframes {
            keyframes_c.push(KeyframeC {
                frame: *f,
                value: *v,
                payload_ptr: if payload.is_empty() {
                    std::ptr::null()
                } else {
                    payload.as_ptr()
                },
                payload_len: payload.len(),
            });
        }

        unsafe { (self.vtable.evaluate)(keyframes_c.as_ptr(), keyframes_c.len(), frame, fallback) }
    }
}

static REGISTRY: OnceLock<Vec<EasingPlugin>> = OnceLock::new();
static CURVE_PRESETS: OnceLock<Mutex<CurveRegistry>> = OnceLock::new();

pub fn load_all(easings_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut plugins = Vec::new();

        let builtin_vtable = unsafe { &*neoutl_easing_standard::neoutl_easing_engine_entry() };
        let meta = unsafe { &*((builtin_vtable.meta)()) };
        let builtin_id = unsafe { CStr::from_ptr(meta.id) }
            .to_string_lossy()
            .into_owned();
        let builtin_name = unsafe { CStr::from_ptr(meta.name) }
            .to_string_lossy()
            .into_owned();
        plugins.push(EasingPlugin {
            id: builtin_id,
            name: builtin_name,
            vtable: builtin_vtable,
            _lib: None,
        });

        let candidates: Vec<PathBuf> = match std::fs::read_dir(easings_dir) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_dylib(p))
                .collect(),
            Err(err) => {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] easings/ 読み込み通知: %{arg0} (組み込み標準エンジンのみ使用)",
                        arg0 = format!("{}", err)
                    )
                );
                Vec::new()
            }
        };

        for path in &candidates {
            match load_one(path) {
                Ok(p) => {
                    if !plugins.iter().any(|existing| existing.id == p.id) {
                        plugins.push(p);
                    }
                }
                Err(err) => {
                    eprintln!(
                        "{}",
                        t!(
                            "[NeoUtl] イージングエンジン読み込み失敗 %{arg0}: %{arg1}",
                            arg1 = format!("{}", err)
                        )
                    );
                }
            }
        }

        plugins.sort_by(|a, b| a.id.cmp(&b.id));
        for plugin in &plugins {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] イージングエンジン登録: %{arg0} (%{arg1})",
                    arg0 = plugin.id,
                    arg1 = plugin.name
                )
            );
        }
        plugins
    });

    CURVE_PRESETS
        .get_or_init(|| Mutex::new(CurveRegistry::load(&default_presets_path(easings_dir))));
}

pub fn registry() -> &'static [EasingPlugin] {
    REGISTRY.get().map_or(&[][..], Vec::as_slice)
}

pub fn by_id(id: &str) -> Option<&'static EasingPlugin> {
    registry()
        .iter()
        .find(|p| p.id == id)
        .or_else(|| registry().iter().find(|p| p.id == "neoutl-easing-standard"))
}

pub fn curve_presets() -> Option<&'static Mutex<CurveRegistry>> {
    CURVE_PRESETS.get()
}

pub fn default_easings_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("easings");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/easings");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("easings")
}

fn load_one(path: &Path) -> Result<EasingPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static EasingEngineVTable = unsafe { &*entry() };
    let meta: &'static EasingEngineMeta = unsafe { &*((vtable.meta)()) };
    let id = unsafe { CStr::from_ptr(meta.id) }
        .to_string_lossy()
        .into_owned();
    let name = unsafe { CStr::from_ptr(meta.name) }
        .to_string_lossy()
        .into_owned();
    Ok(EasingPlugin {
        id,
        name,
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
