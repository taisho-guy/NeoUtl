use libloading::Library;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Toml,
}

pub struct NativeThemePlugin {
    pub vtable: &'static neoutl_theme_api::ThemeVTable,
    _lib: Library,
}

impl NativeThemePlugin {
    pub fn new(vtable: &'static neoutl_theme_api::ThemeVTable, lib: Library) -> Self {
        Self { vtable, _lib: lib }
    }
}

pub enum ThemeSource {
    Data { path: PathBuf, format: DataFormat },
    Native { plugin: NativeThemePlugin },
}

pub struct ThemeEntry {
    pub stable_id: String,
    pub name: String,
    pub source: ThemeSource,
}
