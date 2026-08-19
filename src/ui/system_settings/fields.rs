use crate::localization::tr;
use egui::{Color32, Ui};

fn field_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + 2.0 * ui.spacing().button_padding.y
}

fn field_label(ui: &mut Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(0.0, field_height(ui)),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.label(tr(label)),
    );
}

pub fn name_field(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    field_label(ui, label);
    let changed = ui
        .add_sized(
            egui::vec2(ui.available_width(), field_height(ui)),
            egui::TextEdit::singleline(value).vertical_align(egui::Align::Center),
        )
        .changed();
    ui.end_row();
    changed
}

pub fn toggle_field(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    field_label(ui, label);
    let changed = ui.checkbox(value, "").changed();
    ui.end_row();
    changed
}

pub fn int_field(ui: &mut Ui, label: &str, value: &mut i32, min: i32, max: i32) -> bool {
    field_label(ui, label);
    let changed = ui
        .add(egui::DragValue::new(value).range(min..=max))
        .changed();
    *value = (*value).clamp(min, max);
    ui.end_row();
    changed
}

pub fn float_field(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    field_label(ui, label);
    let changed = ui.add(egui::DragValue::new(value).speed(0.1)).changed();
    ui.end_row();
    changed
}

pub fn choice_field(ui: &mut Ui, label: &str, options: &[String], selected: &mut i32) -> bool {
    field_label(ui, label);
    let mut changed = false;
    ui.allocate_ui_with_layout(
        egui::vec2(0.0, field_height(ui)),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            for (i, opt) in options.iter().enumerate() {
                let i = i as i32;
                let active = i == *selected;
                let text = if active {
                    egui::RichText::new(tr(opt)).color(Color32::WHITE)
                } else {
                    egui::RichText::new(tr(opt))
                };
                if ui.selectable_label(active, text).clicked() {
                    *selected = i;
                    changed = true;
                }
            }
        },
    );
    ui.end_row();
    changed
}
