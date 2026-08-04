//! `properties.slint` PropertyRowの移植。
//!
//! Slint版はSliderがpointer-eventを公開しないため、changed連続発火から150ms無操作で
//! releaseを推定するタイマーを要した。egui::Slider/DragValueはdrag_stopped()/
//! lost_focus()で released を直接検知できるため、本実装はタイマーを持たず
//! イベント駆動でcommit（区間セッション開始）/release（区間セッション終了）を確定する。
//! ドラッグスクラブはegui::DragValueの標準挙動（ドラッグ距離*speed）をそのまま用いる。

use super::segment::Segment;
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

pub fn property_row(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    segment: Segment,
    min: f32,
    max: f32,
) -> RowOutcome {
    let id = ui.make_persistent_id(id_source);
    let (mut left_active, mut right_active) = take_active(id);
    let mut out = RowOutcome::empty();
    let speed = (max - min).max(0.001) / 1000.0;
    let mut start_v = segment.start_value;
    let mut end_v = segment.end_value;

    const VALUE_W: f32 = 70.0;
    const BUTTON_W: f32 = 100.0;
    const ROW_HEIGHT: f32 = 22.0;
    let spacing = ui.spacing().item_spacing.x;
    let fixed_w = VALUE_W * 2.0 + BUTTON_W + spacing * 4.0;
    let slider_w = ((ui.available_width() - fixed_w) / 2.0).max(60.0);

    ui.horizontal(|ui| {
        let slider_l = ui.add_sized(
            [slider_w, ROW_HEIGHT],
            egui::Slider::new(&mut start_v, min..=max)
                .show_value(false)
                .trailing_fill(true),
        );
        if slider_l.changed() {
            if !left_active {
                left_active = true;
                out.start_commit = true;
            }
            out.start_value = Some(start_v.clamp(min, max));
        }
        if slider_l.drag_stopped() {
            left_active = false;
            out.start_release = true;
        }

        let drag_l = ui.add_sized(
            [VALUE_W, ROW_HEIGHT],
            egui::DragValue::new(&mut start_v)
                .range(min..=max)
                .speed(speed),
        );
        if drag_l.changed() {
            if !left_active {
                left_active = true;
                out.start_commit = true;
            }
            out.start_value = Some(start_v.clamp(min, max));
        }
        if drag_l.drag_stopped() || drag_l.lost_focus() {
            left_active = false;
            out.start_release = true;
        }

        if ui
            .add_sized([BUTTON_W, ROW_HEIGHT], egui::Button::new(label).small())
            .clicked()
        {
            out.label_clicked = true;
        }

        let drag_r = ui.add_sized(
            [VALUE_W, ROW_HEIGHT],
            egui::DragValue::new(&mut end_v)
                .range(min..=max)
                .speed(speed),
        );
        if drag_r.changed() {
            if !right_active {
                right_active = true;
                out.end_commit = true;
            }
            out.end_value = Some(end_v.clamp(min, max));
        }
        if drag_r.drag_stopped() || drag_r.lost_focus() {
            right_active = false;
            out.end_release = true;
        }

        let slider_r = ui.add_sized(
            [slider_w, ROW_HEIGHT],
            egui::Slider::new(&mut end_v, min..=max)
                .show_value(false)
                .trailing_fill(true),
        );
        if slider_r.changed() {
            if !right_active {
                right_active = true;
                out.end_commit = true;
            }
            out.end_value = Some(end_v.clamp(min, max));
        }
        if slider_r.drag_stopped() {
            right_active = false;
            out.end_release = true;
        }
    });

    set_active(id, (left_active, right_active));
    out
}
