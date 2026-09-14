use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn font_has_usable_glyphs(font: &font_kit::font::Font) -> bool {
    let ascii_ok = font.glyph_for_char('A').map(|g| g != 0).unwrap_or(false);
    let kana_ok = font.glyph_for_char('あ').map(|g| g != 0).unwrap_or(false);
    ascii_ok || kana_ok
}

pub fn installed_fonts() -> &'static Vec<String> {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| {
        use font_kit::family_name::FamilyName;
        use font_kit::properties::Properties;
        let source = font_kit::source::SystemSource::new();
        let mut names = source.all_families().unwrap_or_default();
        names.sort();
        names.dedup();
        names.retain(|name| {
            let Ok(handle) =
                source.select_best_match(&[FamilyName::Title(name.clone())], &Properties::new())
            else {
                return false;
            };
            let Ok(font) = handle.load() else {
                return false;
            };
            font_has_usable_glyphs(&font)
        });
        names
    })
}

static FONT_BYTES_CACHE: OnceLock<Mutex<HashMap<String, Option<Vec<u8>>>>> = OnceLock::new();

fn bytes_cache() -> &'static Mutex<HashMap<String, Option<Vec<u8>>>> {
    FONT_BYTES_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_family_bytes(family: &str) -> Option<Vec<u8>> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;
    let source = SystemSource::new();
    let handle = source
        .select_best_match(&[FamilyName::Title(family.to_owned())], &Properties::new())
        .ok()?;
    let font = handle.load().ok()?;
    font.copy_font_data().map(|d| d.to_vec())
}

pub fn preload_installed_fonts() {
    let families = installed_fonts();
    let cache = bytes_cache();
    for family in families {
        let bytes = load_family_bytes(family);
        cache.lock().unwrap().insert(family.clone(), bytes);
    }
}
