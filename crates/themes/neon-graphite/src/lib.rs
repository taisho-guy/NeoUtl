use neoutl_theme_api::{
    ENTRY_SYMBOL, FixedColorField, ThemeColorsC, ThemeContext, ThemeMeta, ThemeVTable,
};
use std::os::raw::c_char;
use std::sync::OnceLock;

const STABLE_ID: &str = "neoutl.builtin.neon-graphite\0";
const NAME: &str = "Neon Graphite\0";

fn field(hex: &str) -> FixedColorField {
    let bytes = hex.as_bytes();
    let len = bytes.len();
    assert!(len <= 16, "色コード長が16バイトを超過: {hex}");
    let mut value = [0u8; 16];
    value[..len].copy_from_slice(bytes);
    FixedColorField {
        present: true,
        value,
        len: len as u8,
    }
}

struct SyncThemeMeta(ThemeMeta);
unsafe impl Sync for SyncThemeMeta {}

static META: SyncThemeMeta = SyncThemeMeta(ThemeMeta {
    stable_id: STABLE_ID.as_ptr() as *const c_char,
    name: NAME.as_ptr() as *const c_char,
});

static COLORS: OnceLock<ThemeColorsC> = OnceLock::new();

fn colors_static() -> &'static ThemeColorsC {
    COLORS.get_or_init(|| ThemeColorsC {
        background: field("#0d0f14"),
        surface: field("#161a22"),
        border: field("#2a2f3a"),
        text: field("#e8ecf1"),
        accent: field("#00e5a8"),
    })
}

extern "C" fn theme_meta() -> *const ThemeMeta {
    &META.0 as *const ThemeMeta
}

/// 固定パレットのため ctx（壁紙パス・現在時刻）は参照しない。
/// 動的テーマを実装する場合はここで ctx の値を読み、都度算出した ThemeColorsC を返す。
extern "C" fn theme_compute(_ctx: *const ThemeContext) -> *const ThemeColorsC {
    colors_static() as *const ThemeColorsC
}

static VTABLE: ThemeVTable = ThemeVTable {
    meta: theme_meta,
    compute: theme_compute,
};

#[unsafe(no_mangle)]
pub extern "C" fn neoutl_theme_entry() -> *const ThemeVTable {
    &VTABLE as *const ThemeVTable
}

#[allow(dead_code)]
const _: &[u8] = ENTRY_SYMBOL;
