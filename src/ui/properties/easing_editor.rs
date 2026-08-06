//! `properties.slint` `label-clicked => root.edit-keyframes(...)`の移植先。
//! `Curve_Editor移植計画.md` 4節に対応。左ペイン(キーフレーム/セグメント/
//! モディファイア)・右ペイン(グラフ)の2ペイン構成。
//!
//! カーブは`neoutl-easing-standard`をrlibとして直接呼び出し(FFI非経由)、
//! `ease()`が返す実際の補間値を`egui_plot`でサンプル描画する。
//! Bezierは制御点をegui_plotキャンバス上でドラッグ操作できる。

use crate::easings::loader::curve_presets;
use crate::ecs::EcsWorld;
use crate::ecs::types::{ApplyMode, Keyframe};
use crate::localization::effect_param_label;
use egui_plot::{Line, Plot, PlotPoint, PlotPoints, Points};
use neoutl_easing_standard::{CurveKind, Modifier, ease, encode_payload, parse_payload};
use std::sync::Mutex;

/// 汎用ハンドル種別。1つの`Plot`パッド内で複数ハンドルを最近傍判定するために使う。
#[derive(Clone, Copy, PartialEq, Eq)]
enum HandleId {
    A,
    B,
}

/// ポインタ位置に最も近いハンドルを返す。`HIT_RADIUS_SQ`内でなくても
/// 最近傍側へ常時追従させる(AviUtl側もドラッグ開始点からの継続追従方式)。
fn hit_test(pointer: PlotPoint, a: [f32; 2], b: [f32; 2]) -> HandleId {
    if dist2(pointer, a[0], a[1]) <= dist2(pointer, b[0], b[1]) {
        HandleId::A
    } else {
        HandleId::B
    }
}

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
    selected_frame: Option<i32>,
    preset_name: String,
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
            selected_frame: None,
            preset_name: String::new(),
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

/// AviUtl Curve Editorの「適用モード(標準/補間)」に対応する区間トグル。
fn set_apply_mode(world: &mut EcsWorld, target: &TrackTarget, frame: i32, mode: ApplyMode) {
    match target {
        TrackTarget::Object { object_id, key } => {
            world.set_keyframe_apply_mode(*object_id, key, frame, mode)
        }
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world.set_effect_keyframe_apply_mode(*object_id, *effect_index, key, frame, mode),
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

const KIND_CHOICES: &[&str] = &["Linear", "Bezier", "Bounce", "Elastic", "Normal", "Script"];
const MODIFIER_CHOICES: &[&str] = &["Discretization", "Noise", "SineWave", "SquareWave"];

fn default_for(name: &str) -> CurveKind {
    match name {
        "Bezier" => CurveKind::default_bezier(),
        "Bounce" => CurveKind::default_bounce(),
        "Elastic" => CurveKind::default_elastic(),
        "Normal" => CurveKind::default_normal(),
        "Script" => CurveKind::default_script(),
        _ => CurveKind::Linear,
    }
}

fn default_modifier(name: &str) -> Modifier {
    match name {
        "Noise" => Modifier::Noise {
            seed: 0,
            amplitude: 0.1,
            frequency: 4.0,
            phase: 0.0,
            octaves: 2,
            decay_sharpness: 1.0,
        },
        "SineWave" => Modifier::SineWave {
            amplitude: 0.1,
            frequency: 2.0,
            phase: 0.0,
        },
        "SquareWave" => Modifier::SquareWave {
            amplitude: 0.1,
            frequency: 2.0,
            phase: 0.0,
            duty: 0.5,
        },
        _ => Modifier::Discretization {
            sampling_resolution: 8,
            quantization_resolution: 8,
        },
    }
}

/// `egui_loop.rs`の`WindowKind::EasingEditor`ネイティブウィンドウから毎フレーム呼ばれる。
/// 対象が無ければ即falseを返し、呼び出し側がウィンドウを破棄する。
pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    let Some((target, label, mut selected_frame, mut preset_name_buf)) = ({
        ACTIVE.lock().unwrap().as_ref().map(|s| {
            (
                s.target.clone(),
                s.label.clone(),
                s.selected_frame,
                s.preset_name.clone(),
            )
        })
    }) else {
        return false;
    };
    let track = track_of(world, &target);

    ui.heading(format!("イージング編集: {}", effect_param_label(&label)));
    ui.separator();

    if track.is_empty() {
        ui.weak(t!(
            "キーフレームがありません。プロパティ行の＋KFで追加してください。"
        ));
        return true;
    }

    if selected_frame.is_none_or(|f| {
        !track
            .iter()
            .any(|k| k.frame == f && has_outgoing(&track, f))
    }) {
        selected_frame = track
            .iter()
            .find(|k| has_outgoing(&track, k.frame))
            .map(|k| k.frame);
    }

    let mut removed: Option<i32> = None;
    let mut updated: Option<(i32, i32, f32, String, Vec<u8>)> = None;
    let mut apply_mode_set: Option<(i32, ApplyMode)> = None;
    let mut new_selected = selected_frame;

    ui.columns(2, |cols| {
        let left = &mut cols[0];
        left.label(t!("キーフレーム"));
        egui::Grid::new(("easing_editor_kf_grid", &target))
            .num_columns(3)
            .striped(true)
            .show(left, |ui| {
                for k in &track {
                    let mut frame = k.frame;
                    let mut value = k.value;
                    let selected = selected_frame == Some(k.frame);
                    if ui.selectable_label(selected, "●").clicked() {
                        new_selected = Some(k.frame);
                    }
                    ui.add(egui::DragValue::new(&mut frame).prefix("f:"));
                    ui.add(egui::DragValue::new(&mut value).speed(0.01).prefix("v:"));
                    if ui.small_button("✕").clicked() {
                        removed = Some(k.frame);
                    }
                    ui.end_row();

                    if frame != k.frame || value != k.value {
                        updated = Some((
                            k.frame,
                            frame,
                            value,
                            k.engine_id.clone(),
                            k.engine_payload.clone(),
                        ));
                    }
                }
            });

        left.separator();

        if let Some(sel) = selected_frame.filter(|f| has_outgoing(&track, *f)) {
            let k = track.iter().find(|k| k.frame == sel).unwrap();
            let mut payload = parse_payload(&k.engine_payload);

            left.horizontal(|ui| {
                ui.label(t!("適用モード"));
                let color = match k.apply_mode {
                    ApplyMode::Linear => egui::Color32::from_rgb(0x3b, 0x82, 0xf6),
                    ApplyMode::Interpolate => egui::Color32::from_rgb(0x22, 0xc5, 0x5e),
                };
                let button = egui::Button::new(k.apply_mode.label()).fill(color);
                if ui.add(button).clicked() {
                    apply_mode_set = Some((k.frame, k.apply_mode.toggled()));
                }
            });

            left.label(t!("区間カーブ種別"));
            let mut kind_changed = false;
            egui::ComboBox::new(("easing_kind_combo", &target, sel), "")
                .selected_text(payload.kind.label())
                .show_ui(left, |ui| {
                    for name in KIND_CHOICES {
                        if ui
                            .selectable_label(payload.kind.label() == *name, *name)
                            .clicked()
                            && payload.kind.label() != *name
                        {
                            payload.kind = default_for(name);
                            kind_changed = true;
                        }
                    }
                });

            left.separator();
            edit_kind_params(left, &mut payload.kind, &target, sel);

            left.separator();
            left.label(t!("モディファイア"));
            let mut mod_to_remove: Option<usize> = None;
            for (i, m) in payload.modifiers.iter_mut().enumerate() {
                left.horizontal(|ui| {
                    ui.label(m.label());
                    if ui.small_button("✕").clicked() {
                        mod_to_remove = Some(i);
                    }
                });
                edit_modifier_params(left, m, &target, sel, i);
            }
            if let Some(i) = mod_to_remove {
                payload.modifiers.remove(i);
                kind_changed = true;
            }
            egui::ComboBox::new(
                ("easing_add_modifier", &target, sel),
                t!("＋モディファイア追加"),
            )
            .selected_text("")
            .show_ui(left, |ui| {
                for name in MODIFIER_CHOICES {
                    if ui.selectable_label(false, *name).clicked() {
                        payload.modifiers.push(default_modifier(name));
                        kind_changed = true;
                    }
                }
            });

            left.separator();
            left.label(t!("プリセット"));
            if let Some(reg_mutex) = curve_presets() {
                let mut reg = reg_mutex.lock().unwrap();
                let names: Vec<String> = reg.names().map(str::to_owned).collect();
                egui::ComboBox::new(("easing_preset_apply", &target, sel), t!("適用"))
                    .selected_text("")
                    .show_ui(left, |ui| {
                        for name in &names {
                            if ui.selectable_label(false, name).clicked() {
                                if let Some(k) = reg.get(name) {
                                    payload.kind = k.clone();
                                    kind_changed = true;
                                }
                            }
                        }
                    });
                left.horizontal(|ui| {
                    ui.text_edit_singleline(&mut preset_name_buf);
                    if ui.small_button(t!("現在値を保存")).clicked() && !preset_name_buf.is_empty()
                    {
                        reg.save_as(&preset_name_buf, payload.kind.clone());
                    }
                });
            } else {
                left.weak(t!("プリセットレジストリ未初期化(load_all未実行)"));
            }

            let new_bytes = encode_payload(&payload);
            if kind_changed || new_bytes != k.engine_payload {
                updated = Some((k.frame, k.frame, k.value, k.engine_id.clone(), new_bytes));
            }
        } else {
            left.weak(t!("末尾キーフレームには区間カーブがありません。"));
        }

        let right = &mut cols[1];
        let fit_view = right.button(t!("ビューをフィット")).clicked();
        const SAMPLES: i32 = 64;
        let mut segment_curves: Vec<(Vec<[f64; 2]>, ApplyMode)> = Vec::new();
        for w in track.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let payload = parse_payload(&a.engine_payload);
            let mut curve: Vec<[f64; 2]> = Vec::with_capacity(SAMPLES as usize + 1);
            for s in 0..=SAMPLES {
                let t = s as f32 / SAMPLES as f32;
                let frame = a.frame as f64 + (b.frame - a.frame) as f64 * t as f64;
                let value = a.value + (b.value - a.value) * ease(&payload, t);
                curve.push([frame, value as f64]);
            }
            segment_curves.push((curve, a.apply_mode));
        }
        if segment_curves.is_empty() && !track.is_empty() {
            segment_curves.push((
                vec![[track[0].frame as f64, track[0].value as f64]],
                ApplyMode::Linear,
            ));
        }
        let markers: PlotPoints = track
            .iter()
            .map(|k| [k.frame as f64, k.value as f64])
            .collect();
        let mut plot = Plot::new(("easing_editor_plot", &target)).height(320.0);
        if fit_view {
            plot = plot.reset();
        }
        plot.show(right, |u| {
            for (i, (curve, mode)) in segment_curves.into_iter().enumerate() {
                let color = match mode {
                    ApplyMode::Linear => egui::Color32::from_rgb(0x3b, 0x82, 0xf6),
                    ApplyMode::Interpolate => egui::Color32::from_rgb(0x22, 0xc5, 0x5e),
                };
                let points: PlotPoints = curve.into();
                u.line(Line::new(format!("curve_{i}"), points).color(color));
            }
            u.points(Points::new("keyframes", markers).radius(4.0));
        });
    });

    if let Some(f) = removed {
        remove_kf(world, &target, f);
        if new_selected == Some(f) {
            new_selected = None;
        }
    }
    if let Some((frame, mode)) = apply_mode_set {
        set_apply_mode(world, &target, frame, mode);
    }
    if let Some((old_frame, new_frame, value, engine_id, payload)) = updated {
        if new_frame != old_frame {
            remove_kf(world, &target, old_frame);
        }
        set_kf(world, &target, new_frame, value, engine_id, payload);
        if new_selected == Some(old_frame) {
            new_selected = Some(new_frame);
        }
    }

    if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
        state.selected_frame = new_selected;
        state.preset_name = preset_name_buf.clone();
    }

    ui.separator();
    if ui.button(t!("閉じる")).clicked() {
        close();
    }
    let _ = ctx;
    true
}

fn has_outgoing(track: &[Keyframe], frame: i32) -> bool {
    track
        .iter()
        .position(|k| k.frame == frame)
        .is_some_and(|i| i + 1 < track.len())
}

fn edit_kind_params(ui: &mut egui::Ui, kind: &mut CurveKind, target: &TrackTarget, sel: i32) {
    match kind {
        CurveKind::Linear => {}
        CurveKind::Bezier {
            handle_left,
            handle_right,
        } => {
            ui.horizontal(|ui| {
                ui.small("h1");
                ui.add(
                    egui::DragValue::new(&mut handle_left[0])
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
                ui.add(
                    egui::DragValue::new(&mut handle_left[1])
                        .speed(0.01)
                        .range(-1.0..=2.0),
                );
                ui.small("h2");
                ui.add(
                    egui::DragValue::new(&mut handle_right[0])
                        .speed(0.01)
                        .range(0.0..=1.0),
                );
                ui.add(
                    egui::DragValue::new(&mut handle_right[1])
                        .speed(0.01)
                        .range(-1.0..=2.0),
                );
            });
            Plot::new(("bezier_pad", target, sel))
                .height(90.0)
                .data_aspect(1.0)
                .allow_drag(false)
                .show(ui, |u| {
                    let curve: PlotPoints = (0..=32)
                        .map(|s| {
                            let t = s as f32 / 32.0;
                            let k = CurveKind::Bezier {
                                handle_left: *handle_left,
                                handle_right: *handle_right,
                            };
                            [
                                t as f64,
                                neoutl_easing_standard::curve::evaluate_kind(&k, t) as f64,
                            ]
                        })
                        .collect();
                    u.line(Line::new("bezier", curve));
                    let pts: PlotPoints = vec![
                        [handle_left[0] as f64, handle_left[1] as f64],
                        [handle_right[0] as f64, handle_right[1] as f64],
                    ]
                    .into();
                    u.points(Points::new("cp", pts).radius(5.0));
                    let modifiers = u.ctx().input(|i| i.modifiers);
                    if modifiers.alt && u.response().dragged() {
                        let delta = u.pointer_coordinate_drag_delta();
                        u.translate_bounds(-delta);
                    } else if modifiers.shift && modifiers.ctrl && u.response().dragged() {
                        let delta = u.pointer_coordinate_drag_delta();
                        handle_left[0] = (handle_left[0] + delta.x).clamp(0.0, 1.0);
                        handle_left[1] += delta.y;
                        handle_right[0] = (handle_right[0] + delta.x).clamp(0.0, 1.0);
                        handle_right[1] += delta.y;
                    } else if let Some(pos) = u.pointer_coordinate() {
                        if u.response().dragged() {
                            let mut y = pos.y as f32;
                            if modifiers.shift {
                                y = if y >= 0.5 { 1.0 } else { 0.0 };
                            }
                            match hit_test(pos, *handle_left, *handle_right) {
                                HandleId::A => *handle_left = [pos.x.clamp(0.0, 1.0) as f32, y],
                                HandleId::B => *handle_right = [pos.x.clamp(0.0, 1.0) as f32, y],
                            }
                        }
                    }
                });
        }
        CurveKind::Bounce {
            cor,
            period,
            reversed,
        } => {
            ui.horizontal(|ui| {
                ui.small("cor");
                ui.add(egui::DragValue::new(cor).speed(0.01).range(0.0..=0.99));
                ui.small("period");
                ui.add(egui::DragValue::new(period).speed(0.01).range(0.01..=2.0));
                ui.checkbox(reversed, t!("反転"));
            });
            Plot::new(("bounce_pad", target, sel))
                .height(90.0)
                .allow_drag(false)
                .show(ui, |u| {
                    let curve: PlotPoints = (0..=32)
                        .map(|s| {
                            let t = s as f32 / 32.0;
                            let k = CurveKind::Bounce {
                                cor: *cor,
                                period: *period,
                                reversed: *reversed,
                            };
                            [
                                t as f64,
                                neoutl_easing_standard::curve::evaluate_kind(&k, t) as f64,
                            ]
                        })
                        .collect();
                    u.line(Line::new("bounce", curve));
                    let handle: PlotPoints = vec![[*cor as f64, *period as f64]].into();
                    u.points(Points::new("param", handle).radius(5.0));
                    let modifiers = u.ctx().input(|i| i.modifiers);
                    if modifiers.alt && u.response().dragged() {
                        let delta = u.pointer_coordinate_drag_delta();
                        u.translate_bounds(-delta);
                    } else if let Some(pos) = u.pointer_coordinate() {
                        if u.response().dragged() {
                            *cor = pos.x.clamp(0.0, 0.99) as f32;
                            *period = pos.y.clamp(0.01, 2.0) as f32;
                        }
                    }
                });
        }
        CurveKind::Elastic {
            amplitude,
            frequency,
            decay,
            reversed,
        } => {
            ui.horizontal(|ui| {
                ui.small("amp");
                ui.add(egui::DragValue::new(amplitude).speed(0.01).range(0.0..=4.0));
                ui.small("freq");
                ui.add(
                    egui::DragValue::new(frequency)
                        .speed(0.05)
                        .range(0.1..=16.0),
                );
                ui.small("decay");
                ui.add(egui::DragValue::new(decay).speed(0.05).range(0.0..=16.0));
                ui.checkbox(reversed, t!("反転"));
            });
            Plot::new(("elastic_pad", target, sel))
                .height(90.0)
                .allow_drag(false)
                .show(ui, |u| {
                    let curve: PlotPoints = (0..=32)
                        .map(|s| {
                            let t = s as f32 / 32.0;
                            let k = CurveKind::Elastic {
                                amplitude: *amplitude,
                                frequency: *frequency,
                                decay: *decay,
                                reversed: *reversed,
                            };
                            [
                                t as f64,
                                neoutl_easing_standard::curve::evaluate_kind(&k, t) as f64,
                            ]
                        })
                        .collect();
                    u.line(Line::new("elastic", curve));
                    let handle: PlotPoints = vec![[*frequency as f64, *amplitude as f64]].into();
                    u.points(Points::new("param", handle).radius(5.0));
                    let modifiers = u.ctx().input(|i| i.modifiers);
                    if modifiers.alt && u.response().dragged() {
                        let delta = u.pointer_coordinate_drag_delta();
                        u.translate_bounds(-delta);
                    } else if let Some(pos) = u.pointer_coordinate() {
                        if u.response().dragged() {
                            *frequency = pos.x.clamp(0.1, 16.0) as f32;
                            *amplitude = pos.y.clamp(0.0, 4.0) as f32;
                        }
                    }
                });
        }
        CurveKind::Normal { segments } => {
            ui.label(format!("{}: {}", t!("子セグメント数"), segments.len()));
            ui.horizontal(|ui| {
                if ui.small_button(t!("＋分割追加")).clicked() {
                    neoutl_easing_standard::add_segment(segments, 0.5);
                }
                if segments.len() > 1 && ui.small_button(t!("末尾を削除")).clicked() {
                    let last = segments.len() - 1;
                    neoutl_easing_standard::remove_segment(segments, last);
                }
            });
            for (i, seg) in segments.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.small(format!("[{i}]"));
                    ui.add(
                        egui::DragValue::new(&mut seg.anchor_start[0])
                            .speed(0.01)
                            .range(0.0..=1.0)
                            .prefix("x0:"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut seg.anchor_end[0])
                            .speed(0.01)
                            .range(0.0..=1.0)
                            .prefix("x1:"),
                    );
                    ui.label(seg.kind.label());
                });
            }
            let boundary_count = segments.len().saturating_sub(1);
            {
                Plot::new(("normal_pad", target, sel))
                    .height(90.0)
                    .allow_drag(false)
                    .show(ui, |u| {
                        let curve: PlotPoints = (0..=48)
                            .map(|s| {
                                let t = s as f32 / 48.0;
                                let k = CurveKind::Normal {
                                    segments: segments.clone(),
                                };
                                [
                                    t as f64,
                                    neoutl_easing_standard::curve::evaluate_kind(&k, t) as f64,
                                ]
                            })
                            .collect();
                        u.line(Line::new("normal", curve));
                        let boundary_xs: Vec<f32> = segments[..boundary_count]
                            .iter()
                            .map(|s| s.anchor_end[0])
                            .collect();
                        let handles: PlotPoints =
                            boundary_xs.iter().map(|x| [*x as f64, 0.5f64]).collect();
                        u.points(Points::new("boundaries", handles).radius(5.0));
                        if let Some(pos) = u.pointer_coordinate() {
                            let seg_idx = segments.iter().position(|s| {
                                pos.x as f32 >= s.anchor_start[0] && pos.x as f32 <= s.anchor_end[0]
                            });
                            if let Some(idx) = seg_idx {
                                u.response().context_menu(|menu_ui| {
                                    for name in ["Linear", "Bounce", "Elastic"] {
                                        if menu_ui.button(name).clicked() {
                                            let new_kind = match name {
                                                "Bounce" => CurveKind::default_bounce(),
                                                "Elastic" => CurveKind::default_elastic(),
                                                _ => CurveKind::Linear,
                                            };
                                            neoutl_easing_standard::curve::replace_segment_kind(
                                                segments, idx, new_kind,
                                            );
                                            menu_ui.close();
                                        }
                                    }
                                });
                            }
                        }
                        let modifiers = u.ctx().input(|i| i.modifiers);
                        const HIT_X: f64 = 0.03;
                        let nearest_boundary = u.pointer_coordinate().and_then(|pos| {
                            boundary_xs
                                .iter()
                                .enumerate()
                                .map(|(i, x)| (i, (*x as f64 - pos.x).abs()))
                                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                                .filter(|(_, d)| *d <= HIT_X)
                                .map(|(i, _)| i)
                        });
                        if u.response().double_clicked() {
                            if let Some(i) = nearest_boundary {
                                if segments.len() > 1 {
                                    neoutl_easing_standard::curve::remove_segment(segments, i);
                                }
                            } else if let Some(pos) = u.pointer_coordinate() {
                                let x = pos.x.clamp(0.0, 1.0) as f32;
                                neoutl_easing_standard::curve::add_segment(segments, x);
                            }
                        } else if modifiers.alt && u.response().dragged() {
                            let delta = u.pointer_coordinate_drag_delta();
                            u.translate_bounds(-delta);
                        } else if let Some(pos) = u.pointer_coordinate() {
                            if u.response().dragged()
                                && let Some(i) = nearest_boundary
                            {
                                neoutl_easing_standard::curve::drag_anchor_x(
                                    segments,
                                    i + 1,
                                    pos.x as f32,
                                );
                            }
                        }
                    });
            }
        }
        CurveKind::Script { source } => {
            ui.add(egui::TextEdit::multiline(source).desired_rows(4));
        }
    }
}

fn edit_modifier_params(
    ui: &mut egui::Ui,
    m: &mut Modifier,
    _target: &TrackTarget,
    _sel: i32,
    _idx: usize,
) {
    match m {
        Modifier::Discretization {
            sampling_resolution,
            quantization_resolution,
        } => {
            ui.horizontal(|ui| {
                ui.small("sample");
                ui.add(egui::DragValue::new(sampling_resolution).range(1..=256));
                ui.small("quant");
                ui.add(egui::DragValue::new(quantization_resolution).range(1..=256));
            });
        }
        Modifier::Noise {
            seed,
            amplitude,
            frequency,
            phase,
            octaves,
            decay_sharpness,
        } => {
            ui.horizontal(|ui| {
                ui.small("seed");
                ui.add(egui::DragValue::new(seed));
                ui.small("amp");
                ui.add(egui::DragValue::new(amplitude).speed(0.01));
                ui.small("freq");
                ui.add(egui::DragValue::new(frequency).speed(0.1));
            });
            ui.horizontal(|ui| {
                ui.small("phase");
                ui.add(egui::DragValue::new(phase).speed(0.01));
                ui.small("oct");
                ui.add(egui::DragValue::new(octaves).range(1..=8));
                ui.small("decay");
                ui.add(
                    egui::DragValue::new(decay_sharpness)
                        .speed(0.05)
                        .range(0.01..=8.0),
                );
            });
        }
        Modifier::SineWave {
            amplitude,
            frequency,
            phase,
        } => {
            ui.horizontal(|ui| {
                ui.small("amp");
                ui.add(egui::DragValue::new(amplitude).speed(0.01));
                ui.small("freq");
                ui.add(egui::DragValue::new(frequency).speed(0.1));
                ui.small("phase");
                ui.add(egui::DragValue::new(phase).speed(0.01));
            });
        }
        Modifier::SquareWave {
            amplitude,
            frequency,
            phase,
            duty,
        } => {
            ui.horizontal(|ui| {
                ui.small("amp");
                ui.add(egui::DragValue::new(amplitude).speed(0.01));
                ui.small("freq");
                ui.add(egui::DragValue::new(frequency).speed(0.1));
                ui.small("phase");
                ui.add(egui::DragValue::new(phase).speed(0.01));
                ui.small("duty");
                ui.add(egui::DragValue::new(duty).speed(0.01).range(0.0..=1.0));
            });
        }
    }
}

fn dist2(p: PlotPoint, x: f32, y: f32) -> f64 {
    let dx = p.x - x as f64;
    let dy = p.y - y as f64;
    dx * dx + dy * dy
}
