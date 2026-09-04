use crate::localization::tr;
use egui::Ui;
use elegance::{Select, Slider, SortableItem, SortableList, Switch, TextInput};

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

pub fn float_field(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    field_label(ui, label);
    let changed = ui
        .add(Slider::new(value, f32::MIN..=f32::MAX).step(0.1))
        .changed();
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

pub fn sortable_list_field(
    ui: &mut Ui,
    label: &str,
    id_salt: &str,
    items: &mut Vec<String>,
    item_label: impl Fn(&str) -> String,
) -> bool {
    field_label(ui, label);

    let before = items.clone();
    let mut rows: Vec<SortableItem> = items
        .iter()
        .map(|id| SortableItem::new(id.clone(), item_label(id)))
        .collect();

    ui.vertical(|ui| {
        SortableList::new(ui.id().with(id_salt), &mut rows).show(ui);
    });

    *items = rows.into_iter().map(|row| row.id).collect();
    let changed = *items != before;

    ui.end_row();
    changed
}
