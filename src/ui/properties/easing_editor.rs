//! `properties.slint` `label-clicked => root.edit-keyframes(...)`の移植先。
//!
//! 旧実装は`egui::Window`(同一Contextへの疑似合成ウィンドウ)を用いていたが、
//! 本アプリの他ダイアログは全て`egui_loop.rs`が管理する独立winitネイティブウィンドウ
//! であるため、ここだけ方式が異なっていた(「仮想ウィンドウ形式」)。
//! 本ファイルは編集対象(TrackTarget)の保持と描画のみを担い、ウィンドウの生成/破棄は
//! `egui_loop.rs::WindowKind::EasingEditor`が担当する。
//!
//! カーブは`neoutl-easing-standard`をrlibとして直接呼び出し(FFI非経由)、
//! `ease()`が返す実際の補間値を`egui_plot`でサンプル描画する。
//! Bezierは制御点をegui_plotキャンバス上でドラッグ操作できる。

use crate::ecs::EcsWorld;
use crate::ecs::types::Keyframe;
use crate::localization::effect_param_label;
use egui_plot::{Line, Plot, PlotPoint, PlotPoints, Points};
use neoutl_easing_standard::{StandardEasing, ease, encode_payload, parse_payload};
use std::sync::Mutex;

/// 編集対象トラックの識別子。ホスト側の`EcsWorld`アクセサはオブジェクト系と
/// エフェクト系でシグネチャが異なるため列挙で吸収する。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackTarget {
    Object {
        object_id: usize,
        key: String,
    },
    Effect {
        object_id: usize,
        effect_index: usize,
        key: String,
    },
}

struct EditorState {
    target: TrackTarget,
    label: String,
}

static ACTIVE: Mutex<Option<EditorState>> = Mutex::new(None);

/// ラベルクリック時の起点。同一対象を再クリックした場合は閉じる(トグル)。
pub fn toggle(target: TrackTarget, label: &str) {
    let mut guard = ACTIVE.lock().unwrap();
    let already_this = guard.as_ref().is_some_and(|s| s.target == target);
    *guard = if already_this {
        None
    } else {
        Some(EditorState {
            target,
            label: label.to_owned(),
        })
    };
}

pub fn is_open() -> bool {
    ACTIVE.lock().unwrap().is_some()
}

pub fn close() {
    *ACTIVE.lock().unwrap() = None;
}

fn track_of(world: &EcsWorld, target: &TrackTarget) -> Vec<Keyframe> {
    match target {
        TrackTarget::Object { object_id, key } => world.get_keyframes(*object_id, key),
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world.get_effect_keyframes(*object_id, *effect_index, key),
    }
}

fn set_kf(
    world: &mut EcsWorld,
    target: &TrackTarget,
    frame: i32,
    value: f32,
    engine_id: String,
    payload: Vec<u8>,
) {
    match target {
        TrackTarget::Object { object_id, key } => {
            world.set_keyframe(*object_id, key, frame, value, engine_id, payload)
        }
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world.set_effect_keyframe(
            *object_id,
            *effect_index,
            key,
            frame,
            value,
            engine_id,
            payload,
        ),
    }
}

fn remove_kf(world: &mut EcsWorld, target: &TrackTarget, frame: i32) {
    match target {
        TrackTarget::Object { object_id, key } => world.remove_keyframe(*object_id, key, frame),
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world.remove_effect_keyframe(*object_id, *effect_index, key, frame),
    }
}

const KIND_NAMES: &[(&str, StandardEasing)] = &[
    ("Linear", StandardEasing::Linear),
    ("Step", StandardEasing::Step),
    ("EaseInSine", StandardEasing::EaseInSine),
    ("EaseOutSine", StandardEasing::EaseOutSine),
    ("EaseInOutSine", StandardEasing::EaseInOutSine),
    ("EaseInQuad", StandardEasing::EaseInQuad),
    ("EaseOutQuad", StandardEasing::EaseOutQuad),
    ("EaseInOutQuad", StandardEasing::EaseInOutQuad),
    ("EaseInCubic", StandardEasing::EaseInCubic),
    ("EaseOutCubic", StandardEasing::EaseOutCubic),
    ("EaseInOutCubic", StandardEasing::EaseInOutCubic),
    ("EaseInQuart", StandardEasing::EaseInQuart),
    ("EaseOutQuart", StandardEasing::EaseOutQuart),
    ("EaseInOutQuart", StandardEasing::EaseInOutQuart),
    ("EaseInExpo", StandardEasing::EaseInExpo),
    ("EaseOutExpo", StandardEasing::EaseOutExpo),
    ("EaseInOutExpo", StandardEasing::EaseInOutExpo),
    ("EaseInBack", StandardEasing::EaseInBack),
    ("EaseOutBack", StandardEasing::EaseOutBack),
    ("EaseInOutBack", StandardEasing::EaseInOutBack),
    ("EaseInBounce", StandardEasing::EaseInBounce),
    ("EaseOutBounce", StandardEasing::EaseOutBounce),
    ("EaseInOutBounce", StandardEasing::EaseInOutBounce),
];

fn kind_label(k: &StandardEasing) -> &'static str {
    match k {
        StandardEasing::Bezier { .. } => "Bezier",
        StandardEasing::Random { .. } => "Random",
        other => KIND_NAMES
            .iter()
            .find(|(_, v)| std::mem::discriminant(v) == std::mem::discriminant(other))
            .map(|(n, _)| *n)
            .unwrap_or("Linear"),
    }
}

/// `egui_loop.rs`の`WindowKind::EasingEditor`ネイティブウィンドウから毎フレーム呼ばれる。
/// 対象が無ければ即falseを返し、呼び出し側がウィンドウを破棄する。
pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    let Some(state) = ({
        ACTIVE
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| (s.target.clone(), s.label.clone()))
    }) else {
        return false;
    };
    let (target, label) = state;
    let track = track_of(world, &target);

    ui.heading(format!("イージング編集: {}", effect_param_label(&label)));
    ui.separator();

    if track.is_empty() {
        ui.weak(t!(
            "キーフレームがありません。プロパティ行の＋KFで追加してください。"
        ));
        return true;
    }

    let mut curve: Vec<[f64; 2]> = Vec::new();
    const SAMPLES: i32 = 48;
    for w in track.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let kind = parse_payload(&a.engine_payload);
        for s in 0..=SAMPLES {
            let t = s as f32 / SAMPLES as f32;
            let frame = a.frame as f64 + (b.frame - a.frame) as f64 * t as f64;
            let value = a.value + (b.value - a.value) * ease(kind, t);
            curve.push([frame, value as f64]);
        }
    }
    if curve.is_empty() {
        curve.push([track[0].frame as f64, track[0].value as f64]);
    }
    let points: PlotPoints = curve.into();
    let markers: PlotPoints = track
        .iter()
        .map(|k| [k.frame as f64, k.value as f64])
        .collect();
    Plot::new(("easing_editor_plot", &target))
        .height(160.0)
        .show(ui, |u| {
            u.line(Line::new("curve", points));
            u.points(Points::new("keyframes", markers).radius(4.0));
        });

    ui.separator();
    ui.label(t!("キーフレーム / 区間イージング"));

    let mut removed: Option<i32> = None;
    let mut updated: Option<(i32, i32, f32, String, Vec<u8>)> = None;

    egui::Grid::new(("easing_editor_grid", &target))
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for (i, k) in track.iter().enumerate() {
                let mut frame = k.frame;
                let mut value = k.value;
                let mut kind = parse_payload(&k.engine_payload);

                ui.add(egui::DragValue::new(&mut frame).prefix("f:"));
                ui.add(egui::DragValue::new(&mut value).speed(0.01).prefix("v:"));

                let has_outgoing = i + 1 < track.len();
                ui.add_enabled_ui(has_outgoing, |ui| {
                    egui::ComboBox::new(("easing_kind_combo", &target, k.frame), "")
                        .selected_text(kind_label(&kind))
                        .show_ui(ui, |ui| {
                            for (name, variant) in KIND_NAMES {
                                if ui
                                    .selectable_label(kind_label(&kind) == *name, *name)
                                    .clicked()
                                {
                                    kind = variant.clone();
                                }
                            }
                            if ui
                                .selectable_label(
                                    matches!(kind, StandardEasing::Bezier { .. }),
                                    "Bezier",
                                )
                                .clicked()
                            {
                                kind = StandardEasing::Bezier {
                                    cp1: (0.42, 0.0),
                                    cp2: (0.58, 1.0),
                                };
                            }
                            if ui
                                .selectable_label(
                                    matches!(kind, StandardEasing::Random { .. }),
                                    "Random",
                                )
                                .clicked()
                            {
                                kind = StandardEasing::Random { seed: 0, step: 4 };
                            }
                        });
                });

                if ui.small_button("✕").clicked() {
                    removed = Some(k.frame);
                }
                ui.end_row();

                if has_outgoing {
                    match &mut kind {
                        StandardEasing::Bezier { cp1, cp2 } => {
                            ui.label("");
                            ui.horizontal(|ui| {
                                ui.small("cp1");
                                ui.add(
                                    egui::DragValue::new(&mut cp1.0)
                                        .speed(0.01)
                                        .range(0.0..=1.0),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut cp1.1)
                                        .speed(0.01)
                                        .range(-1.0..=2.0),
                                );
                                ui.small("cp2");
                                ui.add(
                                    egui::DragValue::new(&mut cp2.0)
                                        .speed(0.01)
                                        .range(0.0..=1.0),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut cp2.1)
                                        .speed(0.01)
                                        .range(-1.0..=2.0),
                                );
                            });
                            ui.label("");
                            ui.label("");
                            ui.end_row();

                            Plot::new(("bezier_pad", &target, k.frame))
                                .height(90.0)
                                .data_aspect(1.0)
                                .show(ui, |u| {
                                    let curve: PlotPoints = (0..=32)
                                        .map(|s| {
                                            let t = s as f32 / 32.0;
                                            [
                                                t as f64,
                                                ease(
                                                    StandardEasing::Bezier {
                                                        cp1: *cp1,
                                                        cp2: *cp2,
                                                    },
                                                    t,
                                                )
                                                    as f64,
                                            ]
                                        })
                                        .collect();
                                    u.line(Line::new("bezier", curve));
                                    let pts: PlotPoints = vec![
                                        [cp1.0 as f64, cp1.1 as f64],
                                        [cp2.0 as f64, cp2.1 as f64],
                                    ]
                                    .into();
                                    u.points(Points::new("cp", pts).radius(5.0));
                                    if let Some(pos) = u.pointer_coordinate() {
                                        if u.response().dragged() {
                                            let (d1, d2) = (
                                                dist2(pos, cp1.0, cp1.1),
                                                dist2(pos, cp2.0, cp2.1),
                                            );
                                            if d1 <= d2 {
                                                *cp1 = (pos.x.clamp(0.0, 1.0) as f32, pos.y as f32);
                                            } else {
                                                *cp2 = (pos.x.clamp(0.0, 1.0) as f32, pos.y as f32);
                                            }
                                        }
                                    }
                                });
                            ui.end_row();
                        }
                        StandardEasing::Random { seed, step } => {
                            ui.label("");
                            ui.horizontal(|ui| {
                                let mut seed_i = *seed as i32;
                                ui.small("seed");
                                if ui
                                    .add(egui::DragValue::new(&mut seed_i).range(0..=i32::MAX))
                                    .changed()
                                {
                                    *seed = seed_i as u32;
                                }
                                ui.small("step");
                                ui.add(egui::DragValue::new(step).range(1..=64));
                            });
                            ui.label("");
                            ui.label("");
                            ui.end_row();
                        }
                        _ => {}
                    }
                }

                let payload = encode_payload(kind.clone());
                if frame != k.frame || value != k.value || payload != k.engine_payload {
                    updated = Some((k.frame, frame, value, k.engine_id.clone(), payload));
                }
            }
        });

    if let Some(f) = removed {
        remove_kf(world, &target, f);
    }
    if let Some((old_frame, new_frame, value, engine_id, payload)) = updated {
        if new_frame != old_frame {
            remove_kf(world, &target, old_frame);
        }
        set_kf(world, &target, new_frame, value, engine_id, payload);
    }

    ui.separator();
    if ui.button(t!("閉じる")).clicked() {
        close();
    }
    let _ = ctx;
    true
}

fn dist2(p: PlotPoint, x: f32, y: f32) -> f64 {
    let dx = p.x - x as f64;
    let dy = p.y - y as f64;
    dx * dx + dy * dy
}
