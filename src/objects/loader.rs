use libloading::{Library, Symbol};
use neoutl_object_api::{ENTRY_SYMBOL, EntryFn, ObjectVTable};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub struct ObjectPlugin {
    pub name: String,
    pub kind_id: u32,
    pub vtable: &'static ObjectVTable,
    _lib: Library,
}

static REGISTRY: OnceLock<Vec<ObjectPlugin>> = OnceLock::new();

pub fn load_all(objects_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut plugins = Vec::new();
        let entries = match std::fs::read_dir(objects_dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[NeoUtl] objects/ 読み込み失敗: {err}");
                return plugins;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_dylib(&path) {
                continue;
            }
            match load_one(&path, plugins.len() as u32) {
                Ok(plugin) => {
                    eprintln!(
                        "[NeoUtl] プラグイン登録: {} (kind_id={})",
                        plugin.name, plugin.kind_id
                    );
                    plugins.push(plugin);
                }
                Err(err) => eprintln!("[NeoUtl] プラグイン読み込み失敗 {}: {err}", path.display()),
            }
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

pub fn default_objects_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("objects")))
        .unwrap_or_else(|| PathBuf::from("objects"))
}

fn load_one(path: &Path, kind_id: u32) -> Result<ObjectPlugin, Box<dyn std::error::Error>> {
    let lib = unsafe { Library::new(path) }?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }?;
    let vtable: &'static ObjectVTable = unsafe { &*entry() };
    let meta = unsafe { &*((vtable.meta)()) };
    let name = meta.name.to_owned();
    Ok(ObjectPlugin {
        name,
        kind_id,
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
