use egui::{Color32, Ui};

pub fn name_field(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    ui.label(label);
    let changed = ui.text_edit_singleline(value).changed();
    ui.end_row();
    changed
}

pub fn toggle_field(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    ui.label(label);
    let changed = ui.checkbox(value, "").changed();
    ui.end_row();
    changed
}

pub fn int_field(ui: &mut Ui, label: &str, value: &mut i32, min: i32, max: i32) -> bool {
    ui.label(label);
    let changed = ui
        .add(egui::DragValue::new(value).range(min..=max))
        .changed();
    *value = (*value).clamp(min, max);
    ui.end_row();
    changed
}

pub fn float_field(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    ui.label(label);
    let changed = ui.add(egui::DragValue::new(value).speed(0.1)).changed();
    ui.end_row();
    changed
}

pub fn choice_field(ui: &mut Ui, label: &str, options: &[String], selected: &mut i32) -> bool {
    ui.label(label);
    let mut changed = false;
    ui.horizontal(|ui| {
        for (i, opt) in options.iter().enumerate() {
            let i = i as i32;
            let active = i == *selected;
            let text = if active {
                egui::RichText::new(opt).color(Color32::WHITE)
            } else {
                egui::RichText::new(opt)
            };
            if ui.selectable_label(active, text).clicked() {
                *selected = i;
                changed = true;
            }
        }
    });
    ui.end_row();
    changed
}
