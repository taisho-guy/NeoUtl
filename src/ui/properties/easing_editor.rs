use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{Keyframe, Value};
use crate::localization::effect_param_label;
use egui_plot::{Line, Plot, PlotPoints, Points};
use neoutl_easing_standard::{CurveKind, ease, encode_payload, parse_payload};
use std::sync::Mutex;

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

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    show_curve_editor_layout(ctx, ui, world)
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
