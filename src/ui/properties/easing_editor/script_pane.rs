use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use neoutl_easing_standard::script::evaluate_checked;

pub type Cache = Option<(String, Vec<[f32; 2]>)>;

pub fn validate(source: &str) -> Result<(), String> {
    [0.0, 0.5, 1.0]
        .into_iter()
        .try_for_each(|t| evaluate_checked(source, t).map(|_| ()))
}

pub fn samples<'a>(cache: &'a mut Cache, source: &str) -> &'a [[f32; 2]] {
    if cache.as_ref().is_none_or(|(s, _)| s != source) {
        let points = (0..=96)
            .map(|i| {
                let t = i as f32 / 96.0;
                [t, evaluate_checked(source, t).unwrap_or(t)]
            })
            .collect();
        *cache = Some((source.to_owned(), points));
    }
    cache.as_ref().map_or(&[], |(_, p)| p.as_slice())
}

pub fn show(ui: &mut egui::Ui, buffer: &mut String, error: Option<&str>) -> bool {
    let theme = if ui.visuals().dark_mode {
        ColorTheme::GRUVBOX
    } else {
        ColorTheme::GITHUB_LIGHT
    };
    let changed = CodeEditor::default()
        .id_source("easing_script_editor")
        .with_rows(10)
        .with_fontsize(13.0)
        .with_theme(theme)
        .with_numlines(true)
        .show(ui, buffer, &Syntax::lua())
        .0
        .response
        .changed();
    ui.label(
        egui::RichText::new(
            "変数: t (0〜1), st, ed / 関数: noise(振幅, 周波数, 位相, オクターブ, シード)",
        )
        .weak()
        .small(),
    );
    match error {
        Some(message) => {
            ui.colored_label(ui.visuals().error_fg_color, format!("評価失敗: {message}"));
        }
        None => {
            ui.colored_label(ui.visuals().weak_text_color(), "評価成功");
        }
    }
    changed
}
