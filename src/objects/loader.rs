use libloading::{Library, Symbol};
use neoutl_object_api::{ENTRY_SYMBOL, EntryFn, ObjectVTable};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct ObjectPlugin {
    pub stable_id: String,
    pub name: String,
    pub kind_id: u32,
    pub vtable: &'static ObjectVTable,
    _lib: Library,
}

static REGISTRY: OnceLock<Vec<ObjectPlugin>> = OnceLock::new();

pub fn load_all(objects_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let entries = match std::fs::read_dir(objects_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[NeoUtl] objects/ 読み込み失敗: {err}");
                return Vec::new();
            }
        };
        let candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_dylib(p))
            .collect();

        let mut plugins: Vec<ObjectPlugin> = candidates
            .iter()
            .filter_map(|path| match load_one(path) {
                Ok(p) => Some(p),
                Err(err) => {
                    eprintln!("[NeoUtl] プラグイン読み込み失敗 {}: {err}", path.display());
                    None
                }
            })
            .collect();

        plugins.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        for (kind_id, plugin) in plugins.iter_mut().enumerate() {
            plugin.kind_id = kind_id as u32;
            eprintln!(
                "[NeoUtl] プラグイン登録: {} ({}, kind_id={})",
                plugin.name, plugin.stable_id, plugin.kind_id
            );
        }
        plugins
    });
}

pub fn registry() -> &'static [ObjectPlugin] {
    REGISTRY.get().map(Vec::as_slice).unwrap_or(&[])
}

pub fn by_kind_id(kind_id: u32) -> Option<&'static ObjectPlugin> {
    registry().iter().find(|p| p.kind_id == kind_id)
}

pub fn by_stable_id(stable_id: &str) -> Option<&'static ObjectPlugin> {
    registry().iter().find(|p| p.stable_id == stable_id)
}

pub fn default_objects_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("objects");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/objects");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("objects")
}

fn load_one(path: &Path) -> Result<ObjectPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static ObjectVTable = unsafe { &*entry() };
    let meta = unsafe { &*((vtable.meta)()) };
    Ok(ObjectPlugin {
        stable_id: meta.stable_id.to_owned(),
        name: meta.name.to_owned(),
        kind_id: 0,
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
