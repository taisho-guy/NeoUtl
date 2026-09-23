use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{Keyframe, Value};
use crate::infra::localization::effect_param_label;
use crate::ui::ui_ext::row_style;
use egui_material_icons::icons;
use egui_taffy::{TuiBuilderLogic, tui};
use kurbo::{CubicBez, ParamCurve, Point as KPoint};
use neoutl_easing_standard::{
    ApplyMode, CurveKind, CurveSegment, EasingPayload, ease, encode_payload, parse_payload,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

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
    dragging: Option<usize>,
    selected_point: Option<usize>,
    search: String,
}

static ACTIVE: Mutex<Option<EditorState>> = Mutex::new(None);

static APPLY_ALL_SEGMENTS: AtomicBool = AtomicBool::new(false);

static SESSION_PRESETS: Mutex<Vec<(String, CurveKind)>> = Mutex::new(Vec::new());

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
            dragging: None,
            selected_point: None,
            search: String::new(),
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
    let end = end.max(start + 1);
    let fallback = base_value(world, target);
    let engine = "neoutl-easing-standard".to_owned();
    let payload = encode_payload(&EasingPayload::linear());
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

const CATEGORY_LABELS: [&str; 5] = ["標準", "振動", "バウンス", "スクリプト", "頂点"];

fn category_index(kind: &CurveKind) -> usize {
    match kind {
        CurveKind::Elastic { .. } => 1,
        CurveKind::Bounce { .. } => 2,
        CurveKind::Script { .. } => 3,
        CurveKind::Normal { .. } => 4,
        _ => 0,
    }
}

fn category_default(index: usize) -> CurveKind {
    match index {
        1 => CurveKind::default_elastic(),
        2 => CurveKind::default_bounce(),
        3 => CurveKind::default_script(),
        4 => CurveKind::default_normal(),
        _ => CurveKind::default_bezier(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PointRef {
    BezierLeft,
    BezierRight,
    BounceHandle,
    ElasticAmp,
    ElasticFreqDecay,
    NormalBoundary(usize),
}

fn control_points(kind: &CurveKind) -> Vec<(PointRef, [f32; 2])> {
    match kind {
        CurveKind::Bezier {
            handle_left,
            handle_right,
        } => vec![
            (PointRef::BezierLeft, *handle_left),
            (PointRef::BezierRight, *handle_right),
        ],
        CurveKind::Bounce {
            cor,
            period,
            reversed,
        } => {
            let (x, y) = neoutl_easing_standard::bounce_handle(*cor, *period, *reversed);
            vec![(PointRef::BounceHandle, [x, y])]
        }
        CurveKind::Elastic {
            amplitude,
            frequency,
            decay,
            reversed,
        } => {
            let amp_x = if *reversed { 1.0 } else { 0.0 };
            let amp_y = neoutl_easing_standard::elastic_amp_handle_y(*amplitude);
            let (fx, fy) =
                neoutl_easing_standard::elastic_freq_decay_handle(*frequency, *decay, *reversed);
            vec![
                (PointRef::ElasticAmp, [amp_x, amp_y]),
                (PointRef::ElasticFreqDecay, [fx, fy]),
            ]
        }
        CurveKind::Normal { segments } => segments
            .iter()
            .enumerate()
            .take(segments.len().saturating_sub(1))
            .map(|(i, seg)| (PointRef::NormalBoundary(i), seg.anchor_end))
            .collect(),
        CurveKind::Linear | CurveKind::Standard { .. } | CurveKind::Script { .. } => Vec::new(),
    }
}

fn nearest_control_point(
    points: &[(PointRef, [f32; 2])],
    view: &CurveView,
    screen_pos: egui::Pos2,
    radius: f32,
) -> Option<usize> {
    points
        .iter()
        .map(|(_, p)| view.to_screen(p[0], p[1]).distance(screen_pos))
        .enumerate()
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

fn snap_y(y: f32, shift: bool) -> f32 {
    if shift {
        if y >= 0.5 { 1.0 } else { 0.0 }
    } else {
        y
    }
}

fn apply_point_drag(
    kind: &mut CurveKind,
    point_ref: PointRef,
    data: (f32, f32),
    modifiers: egui::Modifiers,
) {
    let (px, py) = data;
    match (kind, point_ref) {
        (
            CurveKind::Bezier {
                handle_left,
                handle_right,
            },
            PointRef::BezierLeft,
        ) => {
            handle_left[0] = px.clamp(0.0, 1.0);
            handle_left[1] = snap_y(py, modifiers.shift);
            if modifiers.shift && modifiers.ctrl {
                handle_right[0] = (1.0 - handle_left[0]).clamp(0.0, 1.0);
                handle_right[1] = handle_left[1];
            }
        }
        (
            CurveKind::Bezier {
                handle_left,
                handle_right,
            },
            PointRef::BezierRight,
        ) => {
            handle_right[0] = px.clamp(0.0, 1.0);
            handle_right[1] = snap_y(py, modifiers.shift);
            if modifiers.shift && modifiers.ctrl {
                handle_left[0] = (1.0 - handle_right[0]).clamp(0.0, 1.0);
                handle_left[1] = handle_right[1];
            }
        }
        (
            CurveKind::Bounce {
                cor,
                period,
                reversed,
            },
            PointRef::BounceHandle,
        ) => {
            let (new_cor, new_period) =
                neoutl_easing_standard::bounce_set_handle(px, py, *reversed);
            *cor = new_cor;
            *period = new_period;
        }
        (CurveKind::Elastic { amplitude, .. }, PointRef::ElasticAmp) => {
            *amplitude = neoutl_easing_standard::elastic_set_amp(py);
        }
        (
            CurveKind::Elastic {
                frequency,
                decay,
                reversed,
                ..
            },
            PointRef::ElasticFreqDecay,
        ) => {
            let (new_freq, new_decay) =
                neoutl_easing_standard::elastic_set_freq_decay(px, py, *reversed);
            *frequency = new_freq;
            *decay = new_decay;
        }
        (CurveKind::Normal { segments }, PointRef::NormalBoundary(i)) => {
            neoutl_easing_standard::drag_anchor_x(segments, i + 1, px.clamp(0.0, 1.0));
        }
        _ => {}
    }
}

fn merge_boundary(segments: &mut Vec<CurveSegment>, boundary_index: usize) {
    if segments.len() <= 1 || boundary_index + 1 >= segments.len() {
        return;
    }
    let next_end = segments[boundary_index + 1].anchor_end;
    segments[boundary_index].anchor_end = next_end;
    segments.remove(boundary_index + 1);
}

fn reset_point(kind: &mut CurveKind, point_ref: PointRef) -> bool {
    match (kind, point_ref) {
        (CurveKind::Bezier { handle_left, .. }, PointRef::BezierLeft) => {
            *handle_left = [0.42, 0.0];
            true
        }
        (CurveKind::Bezier { handle_right, .. }, PointRef::BezierRight) => {
            *handle_right = [0.58, 1.0];
            true
        }
        (CurveKind::Bounce { cor, period, .. }, PointRef::BounceHandle) => {
            *cor = 0.6;
            *period = 0.5;
            true
        }
        (CurveKind::Elastic { amplitude, .. }, PointRef::ElasticAmp) => {
            *amplitude = 1.0;
            true
        }
        (
            CurveKind::Elastic {
                frequency, decay, ..
            },
            PointRef::ElasticFreqDecay,
        ) => {
            *frequency = 5.0;
            *decay = 6.0;
            true
        }
        (CurveKind::Normal { segments }, PointRef::NormalBoundary(i)) => {
            merge_boundary(segments, i);
            true
        }
        _ => false,
    }
}

pub fn show(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    show_curve_editor_layout(ctx, ui, world)
}

struct CurveView {
    inner: egui::Rect,
}

impl CurveView {
    fn new(rect: egui::Rect, margin: f32) -> Self {
        Self {
            inner: rect.shrink(margin),
        }
    }

    fn to_screen(&self, x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(
            self.inner.left() + self.inner.width() * x,
            self.inner.bottom() - self.inner.height() * y.clamp(-0.3, 1.3),
        )
    }

    fn to_data(&self, p: egui::Pos2) -> (f32, f32) {
        (
            ((p.x - self.inner.left()) / self.inner.width()).clamp(0.0, 1.0),
            (self.inner.bottom() - p.y) / self.inner.height(),
        )
    }
}

fn sample_segment(payload: &EasingPayload, resolution: usize) -> Vec<[f32; 2]> {
    if payload.modifiers.is_empty() {
        if let CurveKind::Bezier {
            handle_left,
            handle_right,
        } = &payload.kind
        {
            let bez = CubicBez::new(
                KPoint::new(0.0, 0.0),
                KPoint::new(handle_left[0] as f64, handle_left[1] as f64),
                KPoint::new(handle_right[0] as f64, handle_right[1] as f64),
                KPoint::new(1.0, 1.0),
            );
            return (0..=resolution)
                .map(|i| {
                    let u = i as f64 / resolution as f64;
                    let p = bez.eval(u);
                    [p.x as f32, p.y as f32]
                })
                .collect();
        }
    }
    (0..=resolution)
        .map(|i| {
            let t = i as f32 / resolution as f32;
            [t, ease(payload, t)]
        })
        .collect()
}

fn show_curve_editor_layout(ctx: &egui::Context, ui: &mut egui::Ui, world: &mut EcsWorld) -> bool {
    let Some((target, label, selected_frame, mut search)) =
        ACTIVE.lock().unwrap().as_ref().map(|s| {
            (
                s.target.clone(),
                s.label.clone(),
                s.selected_frame,
                s.search.clone(),
            )
        })
    else {
        return false;
    };
    let initial = track_of(world, &target);
    ensure_endpoint_keyframes(world, &target, &initial);
    let track = track_of(world, &target);
    let selected = selected_frame.or_else(|| track.windows(2).next().map(|w| w[0].frame));

    let mut close_requested = false;
    let mut curve_changed = false;
    let mut prev_kf_requested = false;
    let mut next_kf_requested = false;
    let mut add_kf_requested = false;

    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    let accent = visuals.selection.bg_fill;
    let weak_text = visuals.weak_text_color();

    let mut active_payload = selected
        .and_then(|frame| track.iter().find(|k| k.frame == frame))
        .map(|k| parse_payload(&k.engine_payload))
        .unwrap_or_else(EasingPayload::linear);
    if matches!(active_payload.kind, CurveKind::Linear) {
        active_payload.kind = CurveKind::default_bezier();
        curve_changed = true;
    }

    tui(ui, ui.id().with("easing_toolbar_row"))
        .style(row_style(4.0))
        .show(|tui| {
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_CONTENT_COPY)
                    .on_hover_text("コピー")
                    .clicked()
                {
                    let json = serde_json::to_string_pretty(&active_payload).unwrap_or_default();
                    ctx.copy_text(json);
                }
            });
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_LIBRARY_ADD)
                    .on_hover_text("保存")
                    .clicked()
                {
                    let mut store = SESSION_PRESETS.lock().unwrap();
                    let name = format!("カスタム {}", store.len() + 1);
                    store.push((name, active_payload.kind.clone()));
                }
            });
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_REFRESH)
                    .on_hover_text("リセット")
                    .clicked()
                {
                    active_payload = EasingPayload::linear();
                    active_payload.kind = CurveKind::default_bezier();
                    curve_changed = true;
                }
            });
            tui.ui(|ui| {
                ui.separator();
            });
            tui.ui(|ui| {
                let category = category_index(&active_payload.kind);
                let mut new_category = category;
                ui.add(
                    elegance::Select::new(("curve_mode", &target), &mut new_category)
                        .options(CATEGORY_LABELS.into_iter().enumerate()),
                );
                if new_category != category {
                    active_payload.kind = category_default(new_category);
                    active_payload.modifiers.clear();
                    curve_changed = true;
                }
            });
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_CHEVRON_LEFT)
                    .on_hover_text("前のキーフレーム")
                    .clicked()
                {
                    prev_kf_requested = true;
                }
            });
            tui.ui(|ui| {
                let position = track
                    .iter()
                    .position(|k| Some(k.frame) == selected)
                    .map(|i| i + 1)
                    .unwrap_or(1);
                ui.label(format!("{position}"));
            });
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_CHEVRON_RIGHT)
                    .on_hover_text("次のキーフレーム")
                    .clicked()
                {
                    next_kf_requested = true;
                }
            });
            tui.ui(|ui| {
                if ui
                    .small_button(icons::ICON_ADD)
                    .on_hover_text("キーフレーム追加")
                    .clicked()
                {
                    add_kf_requested = true;
                }
            });
            tui.ui(|ui| {
                ui.label(egui::RichText::new(effect_param_label(&label)).weak());
            });
        });

    let selected_point_initial = ACTIVE
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|s| s.selected_point);
    tui(ui, ui.id().with("easing_mode_row"))
        .style(row_style(4.0))
        .show(|tui| {
            tui.ui(|ui| {
                let reversible = matches!(
                    active_payload.kind,
                    CurveKind::Bounce { .. } | CurveKind::Elastic { .. }
                );
                if ui
                    .add_enabled(
                        reversible,
                        egui::Button::new(format!("{} 反転", <&str>::from(icons::ICON_SWAP_HORIZ))),
                    )
                    .clicked()
                {
                    match &mut active_payload.kind {
                        CurveKind::Bounce { reversed, .. } => *reversed = !*reversed,
                        CurveKind::Elastic { reversed, .. } => *reversed = !*reversed,
                        _ => {}
                    }
                    curve_changed = true;
                }
            });
            tui.ui(|ui| {
                ui.separator();
            });
            tui.ui(|ui| {
                ui.label("補間モード");
            });
            tui.ui(|ui| {
                const MODES: [ApplyMode; 3] = [
                    ApplyMode::Normal,
                    ApplyMode::IgnoreMidPoint,
                    ApplyMode::Interpolate,
                ];
                let current_mode = MODES
                    .iter()
                    .position(|m| *m == active_payload.apply_mode)
                    .unwrap_or(0);
                let mut new_mode = current_mode;
                ui.add(
                    elegance::Select::new(("apply_mode", &target), &mut new_mode)
                        .options(MODES.iter().map(|m| m.label()).enumerate()),
                );
                if new_mode != current_mode {
                    active_payload.apply_mode = MODES[new_mode];
                    curve_changed = true;
                }
            });
            tui.ui(|ui| {
                ui.separator();
            });
            tui.ui(|ui| {
                let status = match selected_point_initial {
                    Some(i) => format!("選択中の頂点: {}", i + 1),
                    None => "頂点未選択".to_owned(),
                };
                ui.label(egui::RichText::new(status).weak());
            });
        });

    ui.separator();
    ui.columns(2, |cols| {
        let graph_ui = &mut cols[0];
        tui(graph_ui, graph_ui.id().with("easing_graph_header_row"))
            .style(row_style(8.0))
            .show(|tui| {
                tui.ui(|ui| {
                    ui.label(egui::RichText::new("標準").strong());
                });
                tui.ui(|ui| {
                    ui.label("ビュー");
                });
            });
        let selected_index = selected
            .and_then(|frame| track.windows(2).position(|w| w[0].frame == frame))
            .unwrap_or_else(|| track.len().saturating_sub(2));

        let (rect, response) = graph_ui.allocate_exact_size(
            egui::vec2(graph_ui.available_width(), 330.0),
            egui::Sense::click_and_drag(),
        );
        let view = CurveView::new(rect, 14.0);
        let painter = graph_ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
        for i in 0..=8 {
            let x = i as f32 / 8.0;
            painter.line_segment(
                [view.to_screen(x, 0.0), view.to_screen(x, 1.0)],
                egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
            );
        }

        for (segment_index, window) in track.windows(2).enumerate() {
            let active = segment_index == selected_index;
            let segment_payload = if active {
                active_payload.clone()
            } else {
                parse_payload(&window[0].engine_payload)
            };
            let sample = sample_segment(&segment_payload, 128);
            let points: Vec<egui::Pos2> =
                sample.iter().map(|[x, y]| view.to_screen(*x, *y)).collect();
            let color = if active {
                accent
            } else {
                accent.linear_multiply(0.35)
            };
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(if active { 3.0 } else { 1.5 }, color),
            ));
        }

        let control_pts = control_points(&active_payload.kind);
        let (dragging, selected_point) = ACTIVE
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| (s.dragging, s.selected_point))
            .unwrap_or((None, None));
        let highlighted = dragging.or(selected_point);

        match &active_payload.kind {
            CurveKind::Bezier {
                handle_left,
                handle_right,
            } => {
                painter.line_segment(
                    [
                        view.to_screen(0.0, 0.0),
                        view.to_screen(handle_left[0], handle_left[1]),
                    ],
                    egui::Stroke::new(1.0, weak_text),
                );
                painter.line_segment(
                    [
                        view.to_screen(1.0, 1.0),
                        view.to_screen(handle_right[0], handle_right[1]),
                    ],
                    egui::Stroke::new(1.0, weak_text),
                );
            }
            CurveKind::Elastic {
                amplitude,
                reversed,
                ..
            } => {
                let amp_x = if *reversed { 1.0 } else { 0.0 };
                let amp_y = neoutl_easing_standard::elastic_amp_handle_y(*amplitude);
                let base_y = if *reversed { 0.0 } else { 1.0 };
                painter.line_segment(
                    [view.to_screen(amp_x, base_y), view.to_screen(amp_x, amp_y)],
                    egui::Stroke::new(1.0, weak_text),
                );
            }
            CurveKind::Normal { segments } => {
                for seg in segments.iter() {
                    painter.line_segment(
                        [
                            view.to_screen(seg.anchor_start[0], 0.0),
                            view.to_screen(seg.anchor_start[0], 1.0),
                        ],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                    );
                }
            }
            _ => {}
        }
        for (i, (_, p)) in control_pts.iter().enumerate() {
            let color = if Some(i) == highlighted {
                accent
            } else {
                egui::Color32::WHITE
            };
            painter.circle_filled(view.to_screen(p[0], p[1]), 6.0, color);
        }
        painter.circle_filled(view.to_screen(0.0, 0.0), 5.0, egui::Color32::WHITE);
        painter.circle_filled(view.to_screen(1.0, 1.0), 5.0, egui::Color32::WHITE);

        if let CurveKind::Script { source } = &mut active_payload.kind {
            let edit = graph_ui.add(
                egui::TextEdit::multiline(source)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
            if edit.changed() {
                curve_changed = true;
            }
        }

        const HIT_RADIUS: f32 = 10.0;

        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit = nearest_control_point(&control_pts, &view, pos, HIT_RADIUS);
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.dragging = hit;
                    state.selected_point = hit;
                }
            }
        }
        if response.dragged() {
            if let (Some(pos), Some(idx)) = (response.interact_pointer_pos(), dragging) {
                if let Some((point_ref, _)) = control_pts.get(idx).copied() {
                    let modifiers = graph_ui.ctx().input(|i| i.modifiers);
                    let data = view.to_data(pos);
                    apply_point_drag(&mut active_payload.kind, point_ref, data, modifiers);
                    curve_changed = true;
                }
            }
        }
        if response.drag_stopped() {
            if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                state.dragging = None;
            }
        }

        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(idx) = nearest_control_point(&control_pts, &view, pos, HIT_RADIUS) {
                    let point_ref = control_pts[idx].0;
                    if reset_point(&mut active_payload.kind, point_ref) {
                        curve_changed = true;
                    }
                }
            }
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                match nearest_control_point(&control_pts, &view, pos, HIT_RADIUS) {
                    Some(idx) => {
                        if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                            state.selected_point = Some(idx);
                        }
                    }
                    None => {
                        if let CurveKind::Normal { segments } = &mut active_payload.kind {
                            let (x, _) = view.to_data(pos);
                            neoutl_easing_standard::add_segment(segments, x.clamp(0.05, 0.95));
                            curve_changed = true;
                        }
                        if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                            state.selected_point = None;
                        }
                    }
                }
            }
        }

        let preset_ui = &mut cols[1];
        tui(preset_ui, preset_ui.id().with("preset_search_row"))
            .style(row_style(4.0))
            .show(|tui| {
                tui.ui(|ui| {
                    ui.label(icons::ICON_SEARCH);
                });
                tui.style(crate::ui::ui_ext::grow_style()).ui(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut search)
                            .hint_text("プリセットを検索…")
                            .desired_width(f32::INFINITY),
                    );
                });
            });
        preset_ui.separator();
        tui(preset_ui, preset_ui.id().with("preset_heading_row"))
            .style(row_style(4.0))
            .show(|tui| {
                tui.ui(|ui| {
                    ui.label(egui::RichText::new("すべて").strong());
                });
                tui.ui(|ui| {
                    ui.label(format!("(37) {}", <&str>::from(icons::ICON_EXPAND_MORE)));
                });
            });
        preset_ui.label(egui::RichText::new("適用時に現在の頂点構成を上書きします").weak());
        let query = search.to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt(("preset_scroll", &target))
            .show(preset_ui, |ui| {
                let session: Vec<(String, CurveKind)> = SESSION_PRESETS
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(name, _)| query.is_empty() || name.to_lowercase().contains(&query))
                    .cloned()
                    .collect();
                if !session.is_empty() {
                    ui.label(egui::RichText::new("保存済み").weak());
                    for (chunk_index, chunk) in session.chunks(3).enumerate() {
                        tui(ui, ui.id().with(("preset_session_card_row", chunk_index)))
                            .style(row_style(4.0))
                            .show(|tui| {
                                for (name, kind) in chunk {
                                    tui.ui(|ui| {
                                        let response = preset_card(ui, name, kind);
                                        if response.clicked() {
                                            active_payload.kind = kind.clone();
                                            active_payload.modifiers.clear();
                                            curve_changed = true;
                                        }
                                    });
                                }
                            });
                    }
                    ui.separator();
                }
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
                let filtered: Vec<&str> = names
                    .into_iter()
                    .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
                    .collect();
                for (row_index, row) in filtered.chunks(3).enumerate() {
                    tui(ui, ui.id().with(("preset_builtin_card_row", row_index)))
                        .style(row_style(4.0))
                        .show(|tui| {
                            for name in row {
                                tui.ui(|ui| {
                                    let kind = default_for(name);
                                    let response = preset_card(ui, name, &kind);
                                    if response.clicked() {
                                        active_payload.kind = kind;
                                        active_payload.modifiers.clear();
                                        curve_changed = true;
                                    }
                                });
                            }
                        });
                }
            });
    });

    if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
        state.search = search;
    }

    let apply_all = APPLY_ALL_SEGMENTS.load(Ordering::Relaxed);
    let affected = if apply_all { track.len() } else { 1 };
    let mut applied = false;
    ui.add_space(4.0);
    tui(ui, ui.id().with("easing_apply_row"))
        .style(row_style(4.0))
        .show(|tui| {
            tui.style(crate::ui::ui_ext::grow_style()).ui(|ui| {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), 30.0),
                        elegance::Button::new(format!("適用 ({affected})"))
                            .accent(elegance::Accent::Blue),
                    )
                    .clicked()
                {
                    applied = true;
                }
            });
            tui.ui(|ui| {
                if ui
                    .small_button(format!(
                        "{} {}",
                        <&str>::from(icons::ICON_EXPAND_MORE),
                        if apply_all {
                            "全区間"
                        } else {
                            "選択区間"
                        }
                    ))
                    .on_hover_text("適用範囲の切替")
                    .clicked()
                {
                    APPLY_ALL_SEGMENTS.store(!apply_all, Ordering::Relaxed);
                }
            });
            tui.ui(|ui| {
                if ui.small_button("閉じる").clicked() {
                    close_requested = true;
                }
            });
        });

    if prev_kf_requested {
        if let Some(idx) = track.iter().position(|k| Some(k.frame) == selected) {
            if idx > 0 {
                let prev_frame = track[idx - 1].frame;
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.selected_frame = Some(prev_frame);
                    state.selected_point = None;
                }
            }
        }
    }
    if next_kf_requested {
        if let Some(idx) = track.iter().position(|k| Some(k.frame) == selected) {
            let max_idx = track.len().saturating_sub(2);
            if idx < max_idx {
                let next_frame = track[idx + 1].frame;
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.selected_frame = Some(next_frame);
                    state.selected_point = None;
                }
            }
        }
    }
    if add_kf_requested {
        if let Some(idx) = track.iter().position(|k| Some(k.frame) == selected) {
            let new_frame = if idx + 1 < track.len() {
                (track[idx].frame + track[idx + 1].frame) / 2
            } else {
                track[idx].frame + 1
            };
            if new_frame != track[idx].frame && !track.iter().any(|k| k.frame == new_frame) {
                set_kf(
                    world,
                    &target,
                    new_frame,
                    track[idx].value,
                    "neoutl-easing-standard".to_owned(),
                    encode_payload(&EasingPayload::linear()),
                );
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.selected_frame = Some(new_frame);
                    state.selected_point = None;
                }
            }
        }
    }

    if curve_changed {
        if let Some(frame) = selected {
            if let Some(k) = track.iter().find(|k| k.frame == frame) {
                set_kf(
                    world,
                    &target,
                    frame,
                    k.value,
                    k.engine_id.clone(),
                    encode_payload(&active_payload),
                );
            }
        }
    }
    if applied {
        let frames: Vec<i32> = if apply_all {
            track.iter().map(|k| k.frame).collect()
        } else {
            selected.into_iter().collect()
        };
        for frame in frames {
            if let Some(k) = track.iter().find(|k| k.frame == frame) {
                set_kf(
                    world,
                    &target,
                    frame,
                    k.value,
                    k.engine_id.clone(),
                    encode_payload(&active_payload),
                );
            }
        }
    }
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
