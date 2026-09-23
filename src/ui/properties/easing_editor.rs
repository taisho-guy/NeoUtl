use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{Keyframe, Value};
use crate::infra::localization::effect_param_label;
use egui_material_icons::icons;
use kurbo::{CubicBez, ParamCurve, Point as KPoint};
use neoutl_easing_standard::{CurveKind, EasingPayload, ease, encode_payload, parse_payload};
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

    let mut selected_kind: Option<CurveKind> = None;
    let mut close_requested = false;
    let mut curve_changed = false;
    let mut edited_payload = None;
    let mut prev_kf_requested = false;
    let mut add_kf_requested = false;
    let mut copy_requested = false;
    let mut save_requested = false;
    let mut reset_requested = false;
    let visuals = ctx.style_of(ctx.theme()).visuals.clone();
    let accent = visuals.selection.bg_fill;
    let weak_text = visuals.weak_text_color();

    ui.horizontal(|ui| {
        if ui
            .small_button(icons::ICON_CONTENT_COPY)
            .on_hover_text("コピー")
            .clicked()
        {
            copy_requested = true;
        }
        if ui
            .small_button(icons::ICON_LIBRARY_ADD)
            .on_hover_text("保存")
            .clicked()
        {
            save_requested = true;
        }
        if ui
            .small_button(icons::ICON_REFRESH)
            .on_hover_text("リセット")
            .clicked()
        {
            reset_requested = true;
        }
        ui.separator();
        {
            let mut curve_mode_idx: usize = 0;
            ui.add(
                elegance::Select::new(("curve_mode", &target), &mut curve_mode_idx).options(
                    ["標準", "振動", "バウンス", "スクリプト"]
                        .into_iter()
                        .enumerate(),
                ),
            );
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
    let mut reverse_toggled = false;
    ui.horizontal(|ui| {
        if ui
            .small_button(format!("{} 反転", <&str>::from(icons::ICON_SWAP_HORIZ)))
            .clicked()
        {
            reverse_toggled = true;
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
        let mut active_payload = selected
            .and_then(|frame| track.iter().find(|k| k.frame == frame))
            .map(|k| parse_payload(&k.engine_payload))
            .unwrap_or_else(EasingPayload::linear);
        if reverse_toggled {
            match &mut active_payload.kind {
                CurveKind::Bounce { reversed, .. } => *reversed = !*reversed,
                CurveKind::Elastic { reversed, .. } => *reversed = !*reversed,
                _ => {}
            }
            curve_changed = true;
        }
        if reset_requested {
            active_payload = EasingPayload::linear();
            curve_changed = true;
        }
        if copy_requested {
            let json = serde_json::to_string_pretty(&active_payload).unwrap_or_default();
            graph_ui.ctx().copy_text(json);
        }
        if save_requested {
            let mut store = SESSION_PRESETS.lock().unwrap();
            let name = format!("カスタム {}", store.len() + 1);
            store.push((name, active_payload.kind.clone()));
        }

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
            let segment_payload = parse_payload(&window[0].engine_payload);
            let sample = sample_segment(&segment_payload, 128);
            let points: Vec<egui::Pos2> =
                sample.iter().map(|[x, y]| view.to_screen(*x, *y)).collect();
            let active = segment_index == selected_index;
            let color = if active {
                accent
            } else {
                accent.linear_multiply(0.35)
            };
            painter.add(egui::Shape::line(
                points,
                egui::Stroke::new(if active { 3.0 } else { 1.5 }, color),
            ));

            if !active {
                continue;
            }
            if let CurveKind::Bezier {
                handle_left,
                handle_right,
            } = &active_payload.kind
            {
                let hl = view.to_screen(handle_left[0], handle_left[1]);
                let hr = view.to_screen(handle_right[0], handle_right[1]);
                painter.line_segment(
                    [view.to_screen(0.0, 0.0), hl],
                    egui::Stroke::new(1.0, weak_text),
                );
                painter.line_segment(
                    [view.to_screen(1.0, 1.0), hr],
                    egui::Stroke::new(1.0, weak_text),
                );
                painter.circle_filled(hl, 6.0, egui::Color32::WHITE);
                painter.circle_filled(hr, 6.0, egui::Color32::WHITE);
            }
            if let CurveKind::Bounce {
                cor,
                period,
                reversed,
            } = &active_payload.kind
            {
                let (hx, hy) = neoutl_easing_standard::bounce_handle(*cor, *period, *reversed);
                painter.circle_filled(view.to_screen(hx, hy), 6.0, egui::Color32::WHITE);
            }
            if let CurveKind::Elastic {
                amplitude,
                frequency,
                decay,
                reversed,
            } = &active_payload.kind
            {
                let amp_x = if *reversed { 1.0 } else { 0.0 };
                let amp_y = neoutl_easing_standard::elastic_amp_handle_y(*amplitude);
                let base_y = if *reversed { 0.0 } else { 1.0 };
                painter.line_segment(
                    [view.to_screen(amp_x, base_y), view.to_screen(amp_x, amp_y)],
                    egui::Stroke::new(1.0, weak_text),
                );
                let (fx, fy) = neoutl_easing_standard::elastic_freq_decay_handle(
                    *frequency, *decay, *reversed,
                );
                painter.circle_filled(view.to_screen(amp_x, amp_y), 6.0, egui::Color32::WHITE);
                painter.circle_filled(view.to_screen(fx, fy), 6.0, egui::Color32::WHITE);
            }
            painter.circle_filled(view.to_screen(0.0, 0.0), 5.0, egui::Color32::WHITE);
            painter.circle_filled(view.to_screen(1.0, 1.0), 5.0, egui::Color32::WHITE);
        }

        if response.double_clicked() {
            if let CurveKind::Normal { segments } = &mut active_payload.kind {
                if let Some(pos) = response.interact_pointer_pos() {
                    let (x, _y) = view.to_data(pos);
                    neoutl_easing_standard::add_segment(segments, x.clamp(0.05, 0.95));
                    curve_changed = true;
                }
            }
        } else if response.dragged() {
            let modifiers = graph_ui.ctx().input(|i| i.modifiers);
            if let Some(pos) = response.interact_pointer_pos() {
                let (px, py) = view.to_data(pos);
                if let CurveKind::Bezier {
                    handle_left,
                    handle_right,
                } = &mut active_payload.kind
                {
                    let dl = (px - handle_left[0]).powi(2) + (py - handle_left[1]).powi(2);
                    let dr = (px - handle_right[0]).powi(2) + (py - handle_right[1]).powi(2);
                    let snap = |y: f32| {
                        if modifiers.shift {
                            if y >= 0.5 { 1.0 } else { 0.0 }
                        } else {
                            y
                        }
                    };
                    if dl <= dr {
                        handle_left[0] = px.clamp(0.0, 1.0);
                        handle_left[1] = snap(py);
                        if modifiers.shift && modifiers.ctrl {
                            handle_right[0] = (1.0 - handle_left[0]).clamp(0.0, 1.0);
                            handle_right[1] = handle_left[1];
                        }
                    } else {
                        handle_right[0] = px.clamp(0.0, 1.0);
                        handle_right[1] = snap(py);
                        if modifiers.shift && modifiers.ctrl {
                            handle_left[0] = (1.0 - handle_right[0]).clamp(0.0, 1.0);
                            handle_left[1] = handle_right[1];
                        }
                    }
                    curve_changed = true;
                }
                if let CurveKind::Bounce {
                    cor,
                    period,
                    reversed,
                } = &mut active_payload.kind
                {
                    let (new_cor, new_period) =
                        neoutl_easing_standard::bounce_set_handle(px, py, *reversed);
                    *cor = new_cor;
                    *period = new_period;
                    curve_changed = true;
                }
                if let CurveKind::Elastic {
                    amplitude,
                    frequency,
                    decay,
                    reversed,
                } = &mut active_payload.kind
                {
                    let amp_x = if *reversed { 1.0 } else { 0.0 };
                    let dist_to_amp = (px - amp_x).abs();
                    if dist_to_amp < 0.08 {
                        *amplitude = neoutl_easing_standard::elastic_set_amp(py);
                    } else {
                        let (new_freq, new_decay) =
                            neoutl_easing_standard::elastic_set_freq_decay(px, py, *reversed);
                        *frequency = new_freq;
                        *decay = new_decay;
                    }
                    curve_changed = true;
                }
            }
        }
        edited_payload = Some(active_payload);

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
                                    selected_kind = Some(kind.clone());
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

    if let (Some(frame), Some(kind)) = (selected, selected_kind) {
        if let Some(k) = track.iter().find(|k| k.frame == frame) {
            set_kf(
                world,
                &target,
                frame,
                k.value,
                k.engine_id.clone(),
                encode_payload(&EasingPayload {
                    kind,
                    modifiers: Vec::new(),
                    apply_mode: Default::default(),
                }),
            );
        }
    }
    if curve_changed {
        if let Some(frame) = selected {
            if let Some(k) = track.iter().find(|k| k.frame == frame) {
                if let Some(payload) = &edited_payload {
                    set_kf(
                        world,
                        &target,
                        frame,
                        k.value,
                        k.engine_id.clone(),
                        encode_payload(payload),
                    );
                }
            }
        }
    }
    if applied {
        if let Some(payload) = &edited_payload {
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
                        encode_payload(payload),
                    );
                }
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
