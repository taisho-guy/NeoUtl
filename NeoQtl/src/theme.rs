use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInTheme {
    Slate,
    Charcoal,
    Frost,
    Paper,
}

static CURRENT: Mutex<BuiltInTheme> = Mutex::new(BuiltInTheme::Slate);

pub fn current() -> BuiltInTheme {
    *CURRENT.lock().unwrap()
}

pub fn set(theme: BuiltInTheme) {
    *CURRENT.lock().unwrap() = theme;
}

pub fn id_of(theme: BuiltInTheme) -> &'static str {
    match theme {
        BuiltInTheme::Slate => "slate",
        BuiltInTheme::Charcoal => "charcoal",
        BuiltInTheme::Frost => "frost",
        BuiltInTheme::Paper => "paper",
    }
}

pub fn from_id(id: &str) -> BuiltInTheme {
    match id {
        "charcoal" => BuiltInTheme::Charcoal,
        "frost" => BuiltInTheme::Frost,
        "paper" => BuiltInTheme::Paper,
        _ => BuiltInTheme::Slate,
    }
}

pub fn restore(theme_id: &str) {
    set(from_id(theme_id));
}
