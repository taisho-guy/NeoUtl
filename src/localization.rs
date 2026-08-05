use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

static PLUGIN_TRANSLATIONS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
static CURRENT_LOCALE: OnceLock<RwLock<String>> = OnceLock::new();

fn plugin_translations() -> &'static RwLock<HashMap<String, String>> {
    PLUGIN_TRANSLATIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn current_locale() -> &'static RwLock<String> {
    CURRENT_LOCALE.get_or_init(|| RwLock::new("ja".to_owned()))
}

/// Set the locale before any UI is constructed. The Japanese source text is
/// intentionally used as the translation key so plugin metadata can be
/// translated without changing the plugin ABI or its internal keys.
pub fn initialize() {
    let locale = std::env::var("NEOUTL_LOCALE")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LANG"))
        .map(|value| {
            if value.to_ascii_lowercase().starts_with("ja") {
                "ja".to_owned()
            } else {
                "en".to_owned()
            }
        })
        .unwrap_or_else(|_| "ja".to_owned());
    rust_i18n::set_locale(&locale);
    *current_locale().write().expect("locale lock poisoned") = locale;
}

pub fn tr(source: &str) -> String {
    if let Some(value) = plugin_translations()
        .read()
        .expect("translation lock poisoned")
        .get(source)
    {
        return value.clone();
    }
    rust_i18n::t!(source).to_string()
}

/// Load an optional plugin-local catalog from `<plugin>/i18n/<locale>.yml`.
/// Missing or invalid catalogs are ignored so third-party plugins remain
/// compatible when they do not ship translations.
pub fn load_plugin_catalog(plugin_path: &Path) {
    let Some(parent) = plugin_path.parent() else {
        return;
    };
    let locale = current_locale()
        .read()
        .expect("locale lock poisoned")
        .clone();
    let direct = parent.join("i18n").join(format!("{locale}.yml"));
    let nested = parent
        .join("i18n")
        .join(
            plugin_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
        )
        .join(format!("{locale}.yml"));
    let Some(path) = [direct, nested].into_iter().find(|path| path.is_file()) else {
        return;
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(entries) = rust_yaml::from_str::<HashMap<String, String>>(&content) else {
        return;
    };
    plugin_translations()
        .write()
        .expect("translation lock poisoned")
        .extend(entries);
}

pub fn effect_name(source: &str) -> String {
    tr(source)
}

pub fn effect_category(source: &str) -> String {
    tr(source)
}

pub fn effect_param_label(source: &str) -> String {
    tr(source)
}
