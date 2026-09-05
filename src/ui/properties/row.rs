use super::segment::Segment;
use crate::localization::effect_param_label;
use elegance::{Button, ButtonSize, Slider};
use std::collections::HashMap;
use std::sync::Mutex;

static ACTIVE_STATE: Mutex<Option<HashMap<egui::Id, (bool, bool)>>> = Mutex::new(None);

fn take_active(id: egui::Id) -> (bool, bool) {
    let mut guard = ACTIVE_STATE.lock().unwrap();
    *guard
        .get_or_insert_with(HashMap::new)
        .entry(id)
        .or_insert((false, false))
}

fn set_active(id: egui::Id, state: (bool, bool)) {
    let mut guard = ACTIVE_STATE.lock().unwrap();
    guard.get_or_insert_with(HashMap::new).insert(id, state);
}

pub struct RowOutcome {
    pub start_value: Option<f32>,
    pub end_value: Option<f32>,
    pub start_commit: bool,
    pub start_release: bool,
    pub end_commit: bool,
    pub end_release: bool,
    pub label_clicked: bool,
}

impl RowOutcome {
    fn empty() -> Self {
        Self {
            start_value: None,
            end_value: None,
            start_commit: false,
            start_release: false,
            end_commit: false,
            end_release: false,
            label_clicked: false,
        }
    }
}

pub fn button_column_width(ui: &egui::Ui, labels: impl Iterator<Item = String>) -> f32 {
    let theme = elegance::Theme::current(ui.ctx());
    let font_id = egui::FontId::proportional(ButtonSize::Small.font_size(&theme));
    let pad = ButtonSize::Small.padding(&theme);
    labels
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label, font_id.clone(), egui::Color32::WHITE)
                .size()
                .x
                + pad.x * 2.0
        })
        .fold(0.0_f32, f32::max)
}

pub fn property_row(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    segment: Segment,
    min: f32,
    max: f32,
    button_w: f32,
) -> RowOutcome {
    let id = ui.make_persistent_id(id_source);
    let (mut left_active, mut right_active) = take_active(id);
    let mut out = RowOutcome::empty();
    let step = (max - min).max(0.001) / 1000.0;
    let mut start_v = segment.start_value;
    let mut end_v = segment.end_value;

    const BOX_W: f32 = 70.0;
    const SLIDER_MIN_W: f32 = 60.0;

    let theme = elegance::Theme::current(ui.ctx());
    let row_height =
        ButtonSize::Small.font_size(&theme) + 2.0 * ButtonSize::Small.padding(&theme).y;

    let button_text = effect_param_label(label);
    let spacing = ui.spacing().item_spacing.x;
    let fixed_w = BOX_W * 2.0 + button_w + spacing * 4.0;
    let slider_w = ((ui.available_width() - fixed_w) / 2.0).max(SLIDER_MIN_W);

    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = slider_w;

        let slider_l = ui.add_sized(
            [slider_w, row_height],
            Slider::new(&mut start_v, min..=max).show_value(false),
        );
        let box_l = ui.add_sized(
            [BOX_W, row_height],
            egui::DragValue::new(&mut start_v)
                .range(min..=max)
                .speed(step),
        );
        if slider_l.changed() || box_l.changed() {
            if !left_active {
                left_active = true;
                out.start_commit = true;
            }
            out.start_value = Some(start_v.clamp(min, max));
        }
        if slider_l.drag_stopped() || box_l.drag_stopped() || box_l.lost_focus() {
            left_active = false;
            out.start_release = true;
        }

        if ui
            .add(
                Button::new(button_text)
                    .size(ButtonSize::Small)
                    .min_width(button_w),
            )
            .clicked()
        {
            out.label_clicked = true;
        }

        let box_r = ui.add_sized(
            [BOX_W, row_height],
            egui::DragValue::new(&mut end_v)
                .range(min..=max)
                .speed(step),
        );
        let slider_r = ui.add_sized(
            [slider_w, row_height],
            Slider::new(&mut end_v, min..=max).show_value(false),
        );
        if box_r.changed() || slider_r.changed() {
            if !right_active {
                right_active = true;
                out.end_commit = true;
            }
            out.end_value = Some(end_v.clamp(min, max));
        }
        if box_r.drag_stopped() || box_r.lost_focus() || slider_r.drag_stopped() {
            right_active = false;
            out.end_release = true;
        }
    });

    set_active(id, (left_active, right_active));
    out
}
