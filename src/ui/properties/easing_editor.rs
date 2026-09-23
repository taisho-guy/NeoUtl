use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{Keyframe, Value};
use crate::infra::localization::effect_param_label;
use egui_material_icons::icons;
use kurbo::{CubicBez, ParamCurve, Point as KPoint};
use neoutl_easing_standard::{
    CurveKind, CurveSegment, EasingPayload, ease, encode_payload, parse_payload,
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

fn category_index(kind: &CurveKind) -> usize {
    match kind {
        CurveKind::Elastic { .. } => 1,
        CurveKind::Bounce { .. } => 2,
        CurveKind::Script { .. } => 3,
        _ => 0,
    }
}

fn category_default(index: usize) -> CurveKind {
    match index {
        1 => CurveKind::default_elastic(),
        2 => CurveKind::default_bounce(),
        3 => CurveKind::default_script(),
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

    let mut close_requested = false;
    let mut curve_changed = false;
    let mut prev_kf_requested = false;
    let mut add_kf_requested = false;

    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    let accent = visuals.selection.bg_fill;
    let weak_text = visuals.weak_text_color();

    let mut active_payload = selected
        .and_then(|frame| track.iter().find(|k| k.frame == frame))
        .map(|k| parse_payload(&k.engine_payload))
        .unwrap_or_else(EasingPayload::linear);

    ui.horizontal(|ui| {
        if ui
            .small_button(icons::ICON_CONTENT_COPY)
            .on_hover_text("コピー")
            .clicked()
        {
            let json = serde_json::to_string_pretty(&active_payload).unwrap_or_default();
            ctx.copy_text(json);
        }
        if ui
            .small_button(icons::ICON_LIBRARY_ADD)
            .on_hover_text("保存")
            .clicked()
        {
            let mut store = SESSION_PRESETS.lock().unwrap();
            let name = format!("カスタム {}", store.len() + 1);
            store.push((name, active_payload.kind.clone()));
        }
        if ui
            .small_button(icons::ICON_REFRESH)
            .on_hover_text("リセット")
            .clicked()
        {
            active_payload = EasingPayload::linear();
            curve_changed = true;
        }
        ui.separator();
        {
            let category = category_index(&active_payload.kind);
            let mut new_category = category;
            ui.add(
                elegance::Select::new(("curve_mode", &target), &mut new_category).options(
                    ["標準", "振動", "バウンス", "スクリプト"]
                        .into_iter()
                        .enumerate(),
                ),
            );
            if new_category != category {
                active_payload.kind = category_default(new_category);
                active_payload.modifiers.clear();
                curve_changed = true;
            }
        }
        if ui
            .small_button(icons::ICON_CHEVRON_LEFT)
            .on_hover_text("前のキーフレーム")
            .clicked()
        {
            prev_kf_requested = true;
        }
        let position = track
            .iter()
            .position(|k| Some(k.frame) == selected)
            .map(|i| i + 1)
            .unwrap_or(1);
        ui.label(format!("{position}"));
        if ui
            .small_button(icons::ICON_ADD)
            .on_hover_text("キーフレーム追加")
            .clicked()
        {
            add_kf_requested = true;
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(effect_param_label(&label)).weak());
    });
    ui.horizontal(|ui| {
        if ui
            .small_button(format!("{} 反転", <&str>::from(icons::ICON_SWAP_HORIZ)))
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

    ui.separator();
    ui.columns(2, |cols| {
        let graph_ui = &mut cols[0];
        graph_ui.horizontal(|ui| {
            ui.label(egui::RichText::new("標準").strong());
            ui.add_space(8.0);
            ui.label("ビュー");
        });
        let selected_index = selected
            .and_then(|frame| track.windows(2).position(|w| w[0].frame == frame))
            .unwrap_or(0);

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
        let dragging = ACTIVE.lock().unwrap().as_ref().and_then(|s| s.dragging);

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
            let color = if Some(i) == dragging {
                accent
            } else {
                egui::Color32::WHITE
            };
            painter.circle_filled(view.to_screen(p[0], p[1]), 6.0, color);
        }
        painter.circle_filled(view.to_screen(0.0, 0.0), 5.0, egui::Color32::WHITE);
        painter.circle_filled(view.to_screen(1.0, 1.0), 5.0, egui::Color32::WHITE);

        const HIT_RADIUS: f32 = 10.0;

        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let hit = nearest_control_point(&control_pts, &view, pos, HIT_RADIUS);
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.dragging = hit;
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
                    if let (CurveKind::Normal { segments }, PointRef::NormalBoundary(i)) =
                        (&mut active_payload.kind, control_pts[idx].0)
                    {
                        merge_boundary(segments, i);
                        curve_changed = true;
                    }
                }
            }
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if nearest_control_point(&control_pts, &view, pos, HIT_RADIUS).is_none() {
                    if let CurveKind::Normal { segments } = &mut active_payload.kind {
                        let (x, _) = view.to_data(pos);
                        neoutl_easing_standard::add_segment(segments, x.clamp(0.05, 0.95));
                        curve_changed = true;
                    }
                }
            }
        }

        let preset_ui = &mut cols[1];
        preset_ui.horizontal(|ui| {
            ui.label(icons::ICON_SEARCH);
            ui.label(egui::RichText::new("プリセットを検索…").weak());
        });
        preset_ui.separator();
        preset_ui.horizontal(|ui| {
            ui.label(egui::RichText::new("すべて").strong());
            ui.label(format!("(37) {}", <&str>::from(icons::ICON_EXPAND_MORE)));
        });
        preset_ui.label(egui::RichText::new("適用時に現在の頂点構成を上書きします").weak());
        egui::ScrollArea::vertical()
            .id_salt(("preset_scroll", &target))
            .show(preset_ui, |ui| {
                let session = SESSION_PRESETS.lock().unwrap().clone();
                if !session.is_empty() {
                    ui.label(egui::RichText::new("保存済み").weak());
                    for chunk in session.chunks(3) {
                        ui.horizontal(|ui| {
                            for (name, kind) in chunk {
                                let response = preset_card(ui, name, kind);
                                if response.clicked() {
                                    active_payload.kind = kind.clone();
                                    active_payload.modifiers.clear();
                                    curve_changed = true;
                                }
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
                for row in names.chunks(3) {
                    ui.horizontal(|ui| {
                        for name in row {
                            let kind = default_for(name);
                            let response = preset_card(ui, name, &kind);
                            if response.clicked() {
                                active_payload.kind = kind;
                                active_payload.modifiers.clear();
                                curve_changed = true;
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
                elegance::Button::new("適用").accent(elegance::Accent::Blue),
            )
            .clicked()
        {
            applied = true;
        }
        let apply_all = APPLY_ALL_SEGMENTS.load(Ordering::Relaxed);
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
        if ui.small_button("閉じる").clicked() {
            close_requested = true;
        }
    });

    if prev_kf_requested {
        if let Some(idx) = track.iter().position(|k| Some(k.frame) == selected) {
            if idx > 0 {
                let prev_frame = track[idx - 1].frame;
                if let Some(state) = ACTIVE.lock().unwrap().as_mut() {
                    state.selected_frame = Some(prev_frame);
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
        let apply_all = APPLY_ALL_SEGMENTS.load(Ordering::Relaxed);
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
