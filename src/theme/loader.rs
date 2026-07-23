use super::entry::{DataFormat, NativeThemePlugin, ThemeEntry, ThemeSource};
use libloading::{Library, Symbol};
use neoutl_theme_api::{ENTRY_SYMBOL, EntryFn, ThemeColors, ThemeContext, field_to_string};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Vec<ThemeEntry>> = OnceLock::new();

#[derive(serde::Deserialize)]
struct DataThemeFile {
    stable_id: String,
    name: String,
}

pub fn load_all(themes_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut entries: Vec<ThemeEntry> = Vec::new();

        let Ok(dir) = std::fs::read_dir(themes_dir) else {
            eprintln!("[NeoUtl] themes/ 読み込み失敗: {}", themes_dir.display());
            return entries;
        };

        for path in dir.flatten().map(|e| e.path()) {
            match path.extension().and_then(OsStr::to_str) {
                Some("json") | Some("toml") => match load_data_entry(&path) {
                    Ok(entry) => entries.push(entry),
                    Err(err) => eprintln!("[NeoUtl] テーマ読込失敗 {}: {err}", path.display()),
                },
                Some("so") | Some("dylib") | Some("dll") => match load_native_entry(&path) {
                    Ok(entry) => entries.push(entry),
                    Err(err) => eprintln!("[NeoUtl] テーマ読込失敗 {}: {err}", path.display()),
                },
                _ => continue,
            }
        }

        dedup_last_wins(&mut entries);
        entries.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        entries
    });
}

fn dedup_last_wins(entries: &mut Vec<ThemeEntry>) {
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<ThemeEntry> = Vec::with_capacity(entries.len());
    for entry in entries.drain(..).rev() {
        if seen.insert(entry.stable_id.clone()) {
            result.push(entry);
        } else {
            eprintln!(
                "[NeoUtl] テーマstable_id重複、後勝ちで無視: {}",
                entry.stable_id
            );
        }
    }
    result.reverse();
    *entries = result;
}

fn load_data_entry(path: &Path) -> Result<ThemeEntry, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let format = match path.extension().and_then(OsStr::to_str) {
        Some("json") => DataFormat::Json,
        Some("toml") => DataFormat::Toml,
        _ => return Err("未対応拡張子".to_string()),
    };
    let file: DataThemeFile = match format {
        DataFormat::Json => serde_json::from_str(&text).map_err(|e| e.to_string())?,
        DataFormat::Toml => toml::from_str(&text).map_err(|e| e.to_string())?,
    };
    Ok(ThemeEntry {
        stable_id: file.stable_id,
        name: file.name,
        source: ThemeSource::Data {
            path: path.to_path_buf(),
            format,
        },
    })
}

fn load_native_entry(path: &Path) -> Result<ThemeEntry, String> {
    let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
    let entry: Symbol<EntryFn> = unsafe { lib.get(ENTRY_SYMBOL) }.map_err(|e| e.to_string())?;
    let vtable: &'static neoutl_theme_api::ThemeVTable = unsafe { &*entry() };
    let meta = unsafe { &*(vtable.meta)() };
    let stable_id = unsafe { std::ffi::CStr::from_ptr(meta.stable_id) }
        .to_string_lossy()
        .into_owned();
    let name = unsafe { std::ffi::CStr::from_ptr(meta.name) }
        .to_string_lossy()
        .into_owned();
    Ok(ThemeEntry {
        stable_id,
        name,
        source: ThemeSource::Native {
            plugin: NativeThemePlugin::new(vtable, lib),
        },
    })
}

pub fn registry() -> &'static [ThemeEntry] {
    REGISTRY.get().map(Vec::as_slice).unwrap_or(&[])
}

pub fn by_stable_id(id: &str) -> Option<&'static ThemeEntry> {
    registry().iter().find(|e| e.stable_id == id)
}

/// 選択テーマの色トークンを確定する。呼出頻度はテーマ切替時・起動時のみ。
/// Dataは都度パース、Nativeはcompute()呼出。FFI境界(ThemeColorsC)はここで閉じ、
/// 以降Data/Native双方が同一のThemeColors型で扱われる。
pub fn resolve(entry: &ThemeEntry, ctx: &ThemeContext) -> ThemeColors {
    match &entry.source {
        ThemeSource::Data { path, format } => {
            let Ok(text) = std::fs::read_to_string(path) else {
                return ThemeColors::default();
            };
            match format {
                DataFormat::Json => serde_json::from_str(&text).unwrap_or_default(),
                DataFormat::Toml => toml::from_str(&text).unwrap_or_default(),
            }
        }
        ThemeSource::Native { plugin } => {
            let raw = (plugin.vtable.compute)(ctx as *const ThemeContext);
            if raw.is_null() {
                return ThemeColors::default();
            }
            let c = unsafe { &*raw };
            ThemeColors {
                background: field_to_string(&c.background),
                surface: field_to_string(&c.surface),
                border: field_to_string(&c.border),
                text: field_to_string(&c.text),
                accent: field_to_string(&c.accent),
            }
        }
    }
}

pub fn default_themes_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("themes")))
        .unwrap_or_else(|| PathBuf::from("themes"))
}
