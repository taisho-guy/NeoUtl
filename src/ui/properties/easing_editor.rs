//! `properties.slint` `label-clicked => root.edit-keyframes(data.segment-start-frame)`の
//! 移植。対象は常に実在する境界点（segment.start_frame）に固定し、任意再生位置は扱わない。

use crate::ecs::types::Keyframe;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::HashSet;
use std::sync::Mutex;

static OPEN_ROWS: Mutex<Option<HashSet<egui::Id>>> = Mutex::new(None);

pub fn toggle(id: egui::Id) {
    let mut guard = OPEN_ROWS.lock().unwrap();
    let set = guard.get_or_insert_with(HashSet::new);
    if !set.remove(&id) {
        set.insert(id);
    }
}

fn is_open(id: egui::Id) -> bool {
    OPEN_ROWS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.contains(&id))
}

fn close(id: egui::Id) {
    if let Some(set) = OPEN_ROWS.lock().unwrap().as_mut() {
        set.remove(&id);
    }
}

/// 対象トラック全キーフレームをframe/value直接編集する。呼び出し側はActionを
/// 受け取り、自身のworld参照へ1個のFnMutとして適用する（set/remove二重借用回避）。
pub enum Action {
    Set(i32, f32, String, Vec<u8>),
    Remove(i32),
}

pub fn show(
    ctx: &egui::Context,
    id: egui::Id,
    label: &str,
    track: &[Keyframe],
    mut apply: impl FnMut(Action),
) {
    if !is_open(id) {
        return;
    }
    let mut open = true;
    egui::Window::new(format!("イージング編集: {label}"))
        .id(id)
        .open(&mut open)
        .show(ctx, |ui| {
            let points: PlotPoints = track
                .iter()
                .map(|k| [k.frame as f64, k.value as f64])
                .collect();
            Plot::new(id.with("plot"))
                .height(90.0)
                .show(ui, |u| u.line(Line::new(label, points)));

            let mut removed = None;
            let mut edited: Option<(i32, i32, f32)> = None;
            egui::Grid::new(id.with("grid"))
                .num_columns(3)
                .show(ui, |ui| {
                    for k in track {
                        let mut frame = k.frame;
                        let mut value = k.value;
                        ui.add(egui::DragValue::new(&mut frame));
                        ui.add(egui::DragValue::new(&mut value).speed(0.01));
                        if ui.small_button("✕").clicked() {
                            removed = Some(k.frame);
                        }
                        if frame != k.frame || value != k.value {
                            edited = Some((k.frame, frame, value));
                        }
                        ui.end_row();
                    }
                });
            if let Some(f) = removed {
                apply(Action::Remove(f));
            }
            if let Some((old_frame, new_frame, value)) = edited {
                let src = track.iter().find(|k| k.frame == old_frame);
                let (e, p) = src
                    .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
                    .unwrap_or(("neoutl-easing-standard".into(), Vec::new()));
                if new_frame != old_frame {
                    apply(Action::Remove(old_frame));
                }
                apply(Action::Set(new_frame, value, e, p));
            }
        });
    if !open {
        close(id);
    }
}
