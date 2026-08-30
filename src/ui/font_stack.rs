use std::sync::{Mutex, OnceLock};

fn installed_fonts() -> &'static Vec<String> {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let source = font_kit::source::SystemSource::new();
        let mut names = source.all_families().unwrap_or_default();
        names.sort();
        names.dedup();
        names
    })
}

struct UiFontState {
    registered_at: std::collections::HashMap<String, u64>,
    failed: std::collections::HashSet<String>,
    defs: egui::FontDefinitions,
}

static UI_FONT_STATE: Mutex<Option<UiFontState>> = Mutex::new(None);

fn ensure_font_loaded(ctx: &egui::Context, family: &str) -> bool {
    if family.is_empty() {
        return false;
    }
    let current_pass = ctx.cumulative_pass_nr();
    let mut guard = UI_FONT_STATE.lock().unwrap();
    let state = guard.get_or_insert_with(|| UiFontState {
        registered_at: std::collections::HashMap::new(),
        failed: std::collections::HashSet::new(),
        defs: egui::FontDefinitions::default(),
    });
    if state.failed.contains(family) {
        return false;
    }
    if let Some(&pass) = state.registered_at.get(family) {
        return current_pass > pass;
    }
    let Some(bytes) = load_family_bytes(family) else {
        state.failed.insert(family.to_owned());
        return false;
    };
    state
        .defs
        .font_data
        .insert(family.to_owned(), egui::FontData::from_owned(bytes).into());
    state.defs.families.insert(
        egui::FontFamily::Name(family.to_owned().into()),
        vec![family.to_owned()],
    );
    state.registered_at.insert(family.to_owned(), current_pass);
    ctx.set_fonts(state.defs.clone());
    ctx.request_repaint();
    false
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

fn font_label(ui: &egui::Ui, family: &str) -> egui::RichText {
    let resolved = !family.is_empty() && ensure_font_loaded(ui.ctx(), family);
    let text = if family.is_empty() {
        t!("(システム標準)").to_string()
    } else {
        family.to_owned()
    };
    let font_id = if resolved {
        egui::FontId::new(14.0, egui::FontFamily::Name(family.to_owned().into()))
    } else {
        egui::FontId::new(14.0, egui::FontFamily::Proportional)
    };
    egui::RichText::new(text).font(font_id)
}

pub fn font_stack_row(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    current: &mut String,
) -> Option<String> {
    let fonts = installed_fonts();
    let selected_index: usize = fonts
        .iter()
        .position(|f| f == current)
        .map(|i| i + 1)
        .unwrap_or(0);
    let selected_label = font_label(ui, current);
    let mut result: Option<String> = None;
    egui::ComboBox::from_id_salt(id_source)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(selected_index == 0, font_label(ui, ""))
                .clicked()
                && selected_index != 0
            {
                result = Some(String::new());
            }
            for (i, name) in fonts.iter().enumerate() {
                if ui
                    .selectable_label(selected_index == i + 1, font_label(ui, name))
                    .clicked()
                    && selected_index != i + 1
                {
                    result = Some(name.clone());
                }
            }
        });
    result
}
