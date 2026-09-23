use crate::infra::localization::tr;
use egui::Ui;
use elegance::{Select, Slider, Switch, TextInput};

pub fn field_height(ui: &Ui) -> f32 {
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
            TextInput::new(value),
        )
        .changed();
    ui.end_row();
    changed
}

pub fn toggle_field(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    field_label(ui, label);
    let changed = ui.add(Switch::new(value, "")).changed();
    ui.end_row();
    changed
}

pub fn int_field(ui: &mut Ui, label: &str, value: &mut i32, min: i32, max: i32) -> bool {
    field_label(ui, label);
    let changed = ui.add(Slider::new(value, min..=max)).changed();
    *value = (*value).clamp(min, max);
    ui.end_row();
    changed
}

pub fn int_text_field(ui: &mut Ui, label: &str, value: &mut i32, min: i32, max: i32) -> bool {
    field_label(ui, label);
    let id = ui.id().with(label);
    let mut buf = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.to_string());
    let resp = ui.add_sized(
        egui::vec2(ui.available_width(), field_height(ui)),
        TextInput::new(&mut buf),
    );
    let mut changed = false;
    if resp.changed()
        && let Ok(parsed) = buf.trim().parse::<i32>()
    {
        let clamped = parsed.clamp(min, max);
        if clamped != *value {
            *value = clamped;
            changed = true;
        }
    }
    if !resp.has_focus() {
        buf = value.to_string();
    }
    ui.data_mut(|d| d.insert_temp(id, buf));
    ui.end_row();
    changed
}

pub fn float_text_field(ui: &mut Ui, label: &str, value: &mut f32, min: f32, max: f32) -> bool {
    field_label(ui, label);
    let id = ui.id().with(label);
    let mut buf = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.to_string());
    let resp = ui.add_sized(
        egui::vec2(ui.available_width(), field_height(ui)),
        TextInput::new(&mut buf),
    );
    let mut changed = false;
    if resp.changed()
        && let Ok(parsed) = buf.trim().parse::<f32>()
        && parsed.is_finite()
    {
        let clamped = parsed.clamp(min, max);
        if clamped != *value {
            *value = clamped;
            changed = true;
        }
    }
    if !resp.has_focus() {
        buf = value.to_string();
    }
    ui.data_mut(|d| d.insert_temp(id, buf));
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
            let mut idx = (*selected).max(0) as usize;
            let resp = ui.add(
                Select::new((ui.id(), "choice_field"), &mut idx)
                    .options(options.iter().enumerate().map(|(i, o)| (i, tr(o)))),
            );
            if resp.changed() {
                *selected = idx as i32;
                changed = true;
            }
        },
    );
    ui.end_row();
    changed
}
