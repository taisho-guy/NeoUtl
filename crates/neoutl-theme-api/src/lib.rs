use serde::Deserialize;
use std::ffi::c_char;
use std::os::raw::c_void;

pub type StrRef = *const c_char;

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct ThemeColors {
    pub background: Option<String>,
    pub surface: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub accent: Option<String>,
}

#[repr(C)]
pub struct ThemeContext {
    pub wallpaper_path: StrRef,
    pub unix_time_sec: i64,
}

#[repr(C)]
pub struct FixedColorField {
    pub present: bool,
    pub value: [u8; 16],
    pub len: u8,
}

#[repr(C)]
pub struct ThemeColorsC {
    pub background: FixedColorField,
    pub surface: FixedColorField,
    pub border: FixedColorField,
    pub text: FixedColorField,
    pub accent: FixedColorField,
}

#[repr(C)]
pub struct ThemeMeta {
    pub stable_id: StrRef,
    pub name: StrRef,
}

#[repr(C)]
pub struct ThemeVTable {
    pub meta: extern "C" fn() -> *const ThemeMeta,
    pub compute: extern "C" fn(ctx: *const ThemeContext) -> *const ThemeColorsC,
}

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_theme_entry";
pub type EntryFn = extern "C" fn() -> *const ThemeVTable;

pub fn field_to_string(field: &FixedColorField) -> Option<String> {
    if !field.present {
        return None;
    }
    let len = field.len as usize;
    std::str::from_utf8(&field.value[..len])
        .ok()
        .map(str::to_owned)
}

pub type OpaqueEntry = *const c_void;
