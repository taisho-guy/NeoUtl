use libloading::{Library, Symbol};
use neoutl_effect_api::{ENTRY_SYMBOL, EffectVTable, EntryFn};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct EffectPlugin {
    pub id: String,
    pub name: String,
    pub category: String,
    pub vtable: &'static EffectVTable,
    _lib: Library,
}

static REGISTRY: OnceLock<Vec<EffectPlugin>> = OnceLock::new();

pub fn load_all(effects_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let entries = match std::fs::read_dir(effects_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[NeoUtl] effects/ 読み込み失敗: {err}");
                return Vec::new();
            }
        };
        let candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_dylib(p))
            .collect();

        let mut plugins: Vec<EffectPlugin> = candidates
            .iter()
            .filter_map(|path| match load_one(path) {
                Ok(p) => Some(p),
                Err(err) => {
                    eprintln!("[NeoUtl] エフェクト読み込み失敗 {}: {err}", path.display());
                    None
                }
            })
            .collect();

        plugins.sort_by(|a, b| a.id.cmp(&b.id));
        for plugin in &plugins {
            eprintln!("[NeoUtl] エフェクト登録: {} ({})", plugin.name, plugin.id);
        }
        plugins
    });
}

pub fn registry() -> &'static [EffectPlugin] {
    REGISTRY.get().map(Vec::as_slice).unwrap_or(&[])
}

pub fn by_id(id: &str) -> Option<&'static EffectPlugin> {
    registry().iter().find(|p| p.id == id)
}

pub fn default_effects_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("effects");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/effects");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("effects")
}

fn load_one(path: &Path) -> Result<EffectPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static EffectVTable = unsafe { &*entry() };
    let meta = unsafe { &*((vtable.meta)()) };
    Ok(EffectPlugin {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        category: meta.category.to_owned(),
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
