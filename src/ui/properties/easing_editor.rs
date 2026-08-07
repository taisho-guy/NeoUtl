use crate::easings::loader::curve_presets;
use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{ApplyMode, Keyframe, Value};
use crate::localization::effect_param_label;
use egui_plot::{HLine, Line, Plot, PlotPoint, PlotPoints, Points, VLine};
use neoutl_easing_standard::{CurveKind, Modifier, ease, encode_payload, parse_payload};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandleId {
    A,
    B,
}

fn hit_test(pointer: PlotPoint, a: [f32; 2], b: [f32; 2]) -> HandleId {
    if dist2(pointer, a[0], a[1]) <= dist2(pointer, b[0], b[1]) {
        HandleId::A
    } else {
        HandleId::B
    }
}

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

fn clip_bounds(world: &EcsWorld, object_id: usize) -> (i32, i32) {
    world
        .get_timeline_objects()
        .into_iter()
        .find(|o| o.id as usize == object_id)
        .map(|o| (o.start_frame, o.end_frame))
        .unwrap_or((0, 1))
}

fn base_value(world: &EcsWorld, target: &TrackTarget) -> f32 {
    match target {
        TrackTarget::Object { object_id, key } => world
            .get_transform(*object_id)
            .and_then(|v| v.get_param(key))
            .or_else(|| {
                world
                    .get_audio_params(*object_id)
                    .and_then(|v| v.get_param(key))
            })
            .or_else(|| world.get_text(*object_id).and_then(|v| v.get_param(key)))
            .or_else(|| world.get_shape(*object_id).and_then(|v| v.get_param(key)))
            .unwrap_or_default(),
        TrackTarget::Effect {
            object_id,
            effect_index,
            key,
        } => world
            .get_effects(*object_id)
            .get(*effect_index)
            .and_then(|effect| effect.params.get(key))
            .and_then(|param| match param.static_value {
                Value::Number(value) => Some(value),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

fn ensure_endpoint_keyframes(world: &mut EcsWorld, target: &TrackTarget, track: &[Keyframe]) {
    let object_id = match target {
        TrackTarget::Object { object_id, .. } | TrackTarget::Effect { object_id, .. } => *object_id,
    };
    let (start, end) = clip_bounds(world, object_id);
    let fallback = base_value(world, target);
    let engine = "neoutl-easing-standard".to_owned();
    let payload = encode_payload(&neoutl_easing_standard::EasingPayload::linear());
    if !track.iter().any(|k| k.frame == start) {
        set_kf(
            world,
            target,
            start,
            fallback,
            engine.clone(),
            payload.clone(),
        );
    }
    if !track.iter().any(|k| k.frame == end) {
        let end_value = track.last().map(|k| k.value).unwrap_or(fallback);
        set_kf(world, target, end, end_value, engine, payload);
    }
}

const KIND_CHOICES: &[&str] = &["Linear", "Bezier", "Bounce", "Elastic", "Normal", "Script"];
const MODIFIER_CHOICES: &[&str] = &["Discretization", "Noise", "SineWave", "SquareWave"];

fn default_for(name: &str) -> CurveKind {
    if name == "linear" || name.starts_with("ease") {
        return CurveKind::standard(name);
    }
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

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    return show_curve_editor_layout(ctx, ui, world);
    #[allow(unreachable_code)]
    {
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
        let initial_track = track_of(world, &target);
        ensure_endpoint_keyframes(world, &target, &initial_track);
        let track = track_of(world, &target);
        let (clip_start, clip_end) = clip_bounds(
            world,
            match &target {
                TrackTarget::Object { object_id, .. } | TrackTarget::Effect { object_id, .. } => {
                    *object_id
                }
            },
        );

        ui.heading(format!("補間設定: {}", effect_param_label(&label)));
        ui.separator();

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
        let mut catalog_kind: Option<&'static str> = None;
        let mut reset_requested = false;

        ui.horizontal(|ui| {
            if ui.button("⧉").on_hover_text(t!("カーブをコピー")).clicked() {
                if let Some(frame) = selected_frame {
                    if let Some(k) = track.iter().find(|k| k.frame == frame) {
                        ui.ctx()
                            .copy_text(String::from_utf8_lossy(&k.engine_payload).into_owned());
                    }
                }
            }
            if ui
                .button("★")
                .on_hover_text(t!("プリセットとして保存"))
                .clicked()
            {}
            if ui.button("↺").on_hover_text(t!("カーブを初期化")).clicked() {
                reset_requested = true;
            }
            ui.separator();
            ui.label(format!(
                "{}  {}",
                label,
                selected_frame.map_or("-".into(), |f| f.to_string())
            ));
        });

        ui.columns(2, |cols| {
            let left = &mut cols[1];
            left.label(egui::RichText::new(t!("種類")).strong());
            for (category, names) in [
                (t!("基本"), vec!["Linear"]),
                (t!("標準カーブ"), vec!["Bezier", "Normal"]),
                (t!("反動と弾性"), vec!["Bounce", "Elastic"]),
                (t!("特殊"), vec!["Script"]),
            ] {
                left.collapsing(category, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for name in names {
                            let selected = selected_frame
                                .and_then(|f| track.iter().find(|k| k.frame == f))
                                .map(|k| parse_payload(&k.engine_payload).kind.label() == name)
                                .unwrap_or(false);
                            if ui.selectable_label(selected, name).clicked() {
                                catalog_kind = Some(name);
                            }
                        }
                    });
                });
            }
            left.separator();
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
                        let endpoint = k.frame == clip_start || k.frame == clip_end;
                        if ui
                            .add_enabled(!endpoint, egui::Button::new("✕"))
                            .on_hover_text(if endpoint {
                                t!("開始点と終了点は区間の境界のため削除できません").to_string()
                            } else {
                                String::new()
                            })
                            .clicked()
                        {
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
                if reset_requested {
                    payload.kind = CurveKind::Linear;
                    payload.modifiers.clear();
                    kind_changed = true;
                }
                if let Some(name) = catalog_kind {
                    if payload.kind.label() != name {
                        payload.kind = default_for(name);
                        kind_changed = true;
                    }
                }
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
                        if ui.small_button(t!("現在値を保存")).clicked()
                            && !preset_name_buf.is_empty()
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

            let right = &mut cols[0];
            right.horizontal(|ui| {
                ui.label(egui::RichText::new(t!("プレビュー")).strong());
                ui.add_space(8.0);
                ui.label(t!("ズーム:"));
                if ui.small_button("−").clicked() {
                    ui.memory_mut(|m| {
                        m.data
                            .insert_temp(egui::Id::new(("easing_fit", &target)), true)
                    });
                }
                ui.label("100%");
                if ui.small_button("+").clicked() {
                    ui.memory_mut(|m| {
                        m.data
                            .insert_temp(egui::Id::new(("easing_fit", &target)), true)
                    });
                }
                if ui.small_button("1:1").clicked() {
                    ui.memory_mut(|m| {
                        m.data
                            .insert_temp(egui::Id::new(("easing_fit", &target)), true)
                    });
                }
            });
            let fit_view = right.ctx().memory(|m| {
                m.data
                    .get_temp::<bool>(egui::Id::new(("easing_fit", &target)))
                    .unwrap_or(false)
            });
            right.separator();
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
            let mut plot = Plot::new(("easing_editor_plot", &target))
                .height(430.0)
                .data_aspect(1.0)
                .allow_boxed_zoom(false);
            if fit_view {
                plot = plot.reset();
            }
            plot.show(right, |u| {
                u.vline(VLine::new("x=0", 0.0).color(egui::Color32::DARK_GRAY));
                u.vline(VLine::new("x=1", 1.0).color(egui::Color32::DARK_GRAY));
                u.hline(HLine::new("y=0", 0.0).color(egui::Color32::DARK_GRAY));
                u.hline(HLine::new("y=1", 1.0).color(egui::Color32::DARK_GRAY));
                let diagonal: PlotPoints = vec![[0.0, 0.0], [1.0, 1.0]].into();
                u.line(Line::new("linear reference", diagonal).color(egui::Color32::DARK_GRAY));
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
}

fn show_curve_editor_layout(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    let Some((target, label, selected_frame)) = ACTIVE
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| (s.target.clone(), s.label.clone(), s.selected_frame))
    else {
        return false;
    };
    let initial = track_of(world, &target);
    ensure_endpoint_keyframes(world, &target, &initial);
    let track = track_of(world, &target);
    let selected = selected_frame.or_else(|| track.windows(2).next().map(|w| w[0].frame));

    let mut selected_kind: Option<CurveKind> = None;
    let mut close_requested = false;
    let mut fit = false;
    let mut curve_changed = false;
    let mut edited_payload = None;
    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    let accent = visuals.selection.bg_fill;
    let text_color = visuals.text_color();
    let weak_text = visuals.weak_text_color();

    ui.horizontal(|ui| {
        for (icon, tip) in [("□", "コピー"), ("★", "保存"), ("↻", "リセット")] {
            ui.small_button(icon).on_hover_text(tip);
        }
        ui.separator();
        egui::ComboBox::from_id_salt(("curve_mode", &target))
            .selected_text("標準")
            .show_ui(ui, |ui| {
                for mode in ["標準", "振動", "バウンス", "スクリプト"] {
                    let _ = ui.selectable_label(mode == "標準", mode);
                }
            });
        if ui.small_button("‹").clicked() {}
        ui.label("1");
        if ui.small_button("＋").clicked() {}
        ui.add_space(4.0);
        ui.label(egui::RichText::new(effect_param_label(&label)).weak());
    });

    ui.separator();
    ui.columns(2, |cols| {
        let graph_ui = &mut cols[0];
        graph_ui.horizontal(|ui| {
            ui.label(egui::RichText::new("標準").strong());
            ui.add_space(8.0);
            ui.label("ビュー");
            if ui
                .small_button("⛶")
                .on_hover_text("ビューをフィット")
                .clicked()
            {
                fit = true;
            }
        });
        let selected_index = selected
            .and_then(|frame| track.windows(2).position(|w| w[0].frame == frame))
            .unwrap_or(0);
        let mut active_payload = selected
            .and_then(|frame| track.iter().find(|k| k.frame == frame))
            .map(|k| parse_payload(&k.engine_payload))
            .unwrap_or_else(|| neoutl_easing_standard::EasingPayload::linear());
        let mut plot = Plot::new(("curve_editor_graph", &target))
            .height(330.0)
            .data_aspect(1.0)
            .allow_boxed_zoom(false)
            .allow_drag(true)
            .allow_scroll(true);
        if fit {
            plot = plot.reset();
        }
        plot.show(graph_ui, |plot_ui| {
            let grid: PlotPoints = (0..=8)
                .map(|i| {
                    let x = i as f64 / 8.0;
                    [x, x]
                })
                .collect();
            plot_ui.line(Line::new("linear", grid).color(egui::Color32::DARK_GRAY));
            for (segment_index, window) in track.windows(2).enumerate() {
                let segment_payload = parse_payload(&window[0].engine_payload);
                let curve: PlotPoints = (0..=128)
                    .map(|i| {
                        let t = i as f32 / 128.0;
                        [t as f64, ease(&segment_payload, t) as f64]
                    })
                    .collect();
                let active = segment_index == selected_index;
                let color = if active {
                    accent
                } else {
                    accent.linear_multiply(0.35)
                };
                plot_ui.line(
                    Line::new(format!("curve_{segment_index}"), curve)
                        .color(color)
                        .width(if active { 3.0 } else { 1.5 }),
                );
                if active {
                    if let CurveKind::Bezier {
                        handle_left,
                        handle_right,
                    } = &active_payload.kind
                    {
                        let controls: PlotPoints = vec![
                            [handle_left[0] as f64, handle_left[1] as f64],
                            [handle_right[0] as f64, handle_right[1] as f64],
                        ]
                        .into();
                        let tangents: PlotPoints = vec![
                            [0.0, 0.0],
                            [handle_left[0] as f64, handle_left[1] as f64],
                            [1.0, 1.0],
                            [handle_right[0] as f64, handle_right[1] as f64],
                        ]
                        .into();
                        plot_ui.line(Line::new("tangents", tangents).color(weak_text));
                        plot_ui.points(
                            Points::new("control_points", controls)
                                .color(egui::Color32::WHITE)
                                .radius(6.0),
                        );
                    }
                    if let CurveKind::Bounce { cor, period, .. } = &active_payload.kind {
                        let point: PlotPoints =
                            vec![[*cor as f64, (*period / 2.0).clamp(0.0, 1.0) as f64]].into();
                        plot_ui.points(
                            Points::new("bounce_handle", point)
                                .color(egui::Color32::WHITE)
                                .radius(6.0),
                        );
                    }
                    if let CurveKind::Elastic {
                        amplitude,
                        frequency,
                        decay,
                        ..
                    } = &active_payload.kind
                    {
                        let points: PlotPoints = vec![
                            [
                                (*frequency / 16.0).clamp(0.0, 1.0) as f64,
                                (*amplitude / 4.0).clamp(0.0, 1.0) as f64,
                            ],
                            [(*decay / 16.0).clamp(0.0, 1.0) as f64, 0.5],
                        ]
                        .into();
                        plot_ui.points(
                            Points::new("elastic_handles", points)
                                .color(egui::Color32::WHITE)
                                .radius(6.0),
                        );
                    }
                    let endpoints: PlotPoints = vec![[0.0, 0.0], [1.0, 1.0]].into();
                    plot_ui.points(
                        Points::new("active_endpoints", endpoints)
                            .color(egui::Color32::WHITE)
                            .radius(5.0),
                    );
                }
            }
            let response = plot_ui.response();
            if response.double_clicked() {
                if let CurveKind::Normal { segments } = &mut active_payload.kind {
                    if let Some(pos) = plot_ui.pointer_coordinate() {
                        let x = pos.x.clamp(0.05, 0.95) as f32;
                        neoutl_easing_standard::add_segment(segments, x);
                        curve_changed = true;
                    }
                }
            } else if response.dragged() {
                let modifiers = plot_ui.ctx().input(|i| i.modifiers);
                if let Some(pos) = plot_ui.pointer_coordinate() {
                    if let CurveKind::Bezier {
                        handle_left,
                        handle_right,
                    } = &mut active_payload.kind
                    {
                        let dl = (pos.x - handle_left[0] as f64).powi(2)
                            + (pos.y - handle_left[1] as f64).powi(2);
                        let dr = (pos.x - handle_right[0] as f64).powi(2)
                            + (pos.y - handle_right[1] as f64).powi(2);
                        let snap = |y: f64| {
                            if modifiers.shift {
                                if y >= 0.5 { 1.0 } else { 0.0 }
                            } else {
                                y
                            }
                        };
                        if dl <= dr {
                            handle_left[0] = pos.x.clamp(0.0, 1.0) as f32;
                            handle_left[1] = snap(pos.y) as f32;
                            if modifiers.shift && modifiers.ctrl {
                                handle_right[0] = (1.0 - handle_left[0]).clamp(0.0, 1.0);
                                handle_right[1] = handle_left[1];
                            }
                        } else {
                            handle_right[0] = pos.x.clamp(0.0, 1.0) as f32;
                            handle_right[1] = snap(pos.y) as f32;
                            if modifiers.shift && modifiers.ctrl {
                                handle_left[0] = (1.0 - handle_right[0]).clamp(0.0, 1.0);
                                handle_left[1] = handle_right[1];
                            }
                        }
                        curve_changed = true;
                    }
                    if let CurveKind::Bounce { cor, period, .. } = &mut active_payload.kind {
                        *cor = pos.x.clamp(0.0, 0.99) as f32;
                        *period = (pos.y.clamp(0.0, 1.0) as f32 * 2.0).clamp(0.01, 2.0);
                        curve_changed = true;
                    }
                    if let CurveKind::Elastic {
                        amplitude,
                        frequency,
                        decay,
                        ..
                    } = &mut active_payload.kind
                    {
                        let modifiers = plot_ui.ctx().input(|i| i.modifiers);
                        if modifiers.ctrl {
                            *decay = (pos.x.clamp(0.0, 1.0) as f32 * 16.0).clamp(0.0, 16.0);
                        } else {
                            *frequency = (pos.x.clamp(0.0, 1.0) as f32 * 16.0).clamp(0.1, 16.0);
                            *amplitude = (pos.y.clamp(0.0, 1.0) as f32 * 4.0).clamp(0.0, 4.0);
                        }
                        curve_changed = true;
                    }
                }
            }
        });
        edited_payload = Some(active_payload);

        let preset_ui = &mut cols[1];
        preset_ui.horizontal(|ui| {
            ui.label(egui::RichText::new("プリセットを検索…").weak());
            ui.label("☷");
        });
        preset_ui.separator();
        preset_ui.horizontal(|ui| {
            ui.label(egui::RichText::new("すべて").strong());
            ui.label("(37)⌄");
        });
        egui::ScrollArea::vertical()
            .id_salt(("preset_scroll", &target))
            .show(preset_ui, |ui| {
                let names = [
                    "linear",
                    "easeInSine",
                    "easeOutSine",
                    "easeInOutSine",
                    "easeOutInSine",
                    "easeInQuad",
                    "easeOutQuad",
                    "easeInOutQuad",
                    "easeOutInQuad",
                    "easeInCubic",
                    "easeOutCubic",
                    "easeInOutCubic",
                    "easeOutInCubic",
                    "easeInQuart",
                    "easeOutQuart",
                    "easeInOutQuart",
                    "easeOutInQuart",
                    "easeInQuint",
                    "easeOutQuint",
                    "easeInOutQuint",
                    "easeOutInQuint",
                    "easeInExpo",
                    "easeOutExpo",
                    "easeInOutExpo",
                    "easeOutInExpo",
                    "easeInCirc",
                    "easeOutCirc",
                    "easeInOutCirc",
                    "easeOutInCirc",
                    "easeInBack",
                    "easeOutBack",
                    "easeInOutBack",
                    "easeOutInBack",
                    "easeInElastic",
                    "easeOutElastic",
                    "easeInBounce",
                    "easeOutBounce",
                ];
                for row in names.chunks(3) {
                    ui.horizontal(|ui| {
                        for name in row {
                            let kind = default_for(name);
                            let response = preset_card(ui, name, &kind);
                            if response.clicked() {
                                selected_kind = Some(kind);
                            }
                        }
                    });
                }
            });
    });

    let mut applied = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                egui::vec2(ui.available_size_before_wrap().x, 30.0),
                egui::Button::new(
                    egui::RichText::new("適用")
                        .color(egui::Color32::WHITE)
                        .strong(),
                )
                .fill(egui::Color32::from_rgb(0x2d, 0x76, 0xb8)),
            )
            .clicked()
        {
            applied = true;
        }
        if ui.small_button("⌄").clicked() {}
        if ui.small_button("閉じる").clicked() {
            close_requested = true;
        }
    });

    if let (Some(frame), Some(kind)) = (selected, selected_kind) {
        if let Some(k) = track.iter().find(|k| k.frame == frame) {
            set_kf(
                world,
                &target,
                frame,
                k.value,
                k.engine_id.clone(),
                encode_payload(&neoutl_easing_standard::EasingPayload {
                    kind,
                    modifiers: Vec::new(),
                }),
            );
        }
    }
    if curve_changed {
        if let Some(frame) = selected {
            if let Some(k) = track.iter().find(|k| k.frame == frame) {
                if let Some(payload) = edited_payload {
                    set_kf(
                        world,
                        &target,
                        frame,
                        k.value,
                        k.engine_id.clone(),
                        encode_payload(&payload),
                    );
                }
            }
        }
    }
    if applied {}
    if close_requested {
        close();
    }
    let _ = ctx;
    true
}

fn preset_card(ui: &mut egui::Ui, name: &str, kind: &CurveKind) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(78.0, 72.0), egui::Sense::click());
    let chart = egui::Rect::from_min_max(
        rect.min + egui::vec2(4.0, 4.0),
        egui::pos2(rect.max.x - 4.0, rect.min.y + 48.0),
    );
    let painter = ui.painter();
    let fill = if response.hovered() {
        egui::Color32::from_rgb(0x35, 0x35, 0x3b)
    } else {
        egui::Color32::from_rgb(0x25, 0x25, 0x2a)
    };
    painter.rect_filled(rect, 4.0, fill);
    painter.rect_stroke(
        chart,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(75)),
        egui::StrokeKind::Inside,
    );
    let points: Vec<egui::Pos2> = (0..=32)
        .map(|i| {
            let t = i as f32 / 32.0;
            let y = evaluate_kind(kind, t);
            egui::pos2(
                chart.left() + chart.width() * t,
                chart.bottom() - chart.height() * y.clamp(-0.2, 1.2),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(0xc8, 0xc8, 0xd0)),
    ));
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 13.0),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(205),
    );
    response
}

fn evaluate_kind(kind: &CurveKind, t: f32) -> f32 {
    neoutl_easing_standard::curve::evaluate_kind(kind, t)
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
        CurveKind::Standard { .. } => {}
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
