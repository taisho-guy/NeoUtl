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
}

pub fn tr(source: &str) -> String {
    rust_i18n::t!(source).to_string()
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
