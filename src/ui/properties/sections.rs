use super::row::{color_row, property_row};
use super::segment::resolve_segment;
use super::track::{keyframe_track, keyframe_track_colored};
use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::object_schema::{
    AUDIO_SCHEMA, CLIP_TARGET_ENABLED_KEY, CLIP_TARGET_SCHEMA, ColorParamSchema,
    GROUP_CONTROL_SCHEMA, SHAPE_COLOR_SCHEMA, SHAPE_SCHEMA, TEXT_SCHEMA, TRANSFORM_SCHEMA,
    is_visible, resolve_range,
};
use crate::ecs::track::Track;
use crate::infra::localization::effect_param_label;
use crate::ui::ui_ext::row_style;
use egui_taffy::{TuiBuilderLogic, tui};
use elegance::{Checkbox, Select, Slider, TextArea};
use neoutl_shared_abi::ParamKind;

pub(super) struct FloatRowCtx<'a, S: std::hash::Hash + Copy + std::fmt::Debug> {
    pub id_source: S,
    pub target: super::easing_editor::TrackTarget,
    pub label: &'a str,
    pub min: f32,
    pub max: f32,
    pub clip_start: i32,
    pub clip_end: i32,
    pub current_frame: i32,
    pub base_value: f32,
    pub track: &'a [crate::ecs::types::Keyframe],
    pub button_w: f32,
}

pub(super) fn float_row<S: std::hash::Hash + Copy + std::fmt::Debug>(
    ui: &mut egui::Ui,
    world: &mut EcsWorld,
    ctx: FloatRowCtx<S>,
    mut set_kf: impl FnMut(&mut EcsWorld, i32, f32, String, Vec<u8>),
    mut remove_kf: impl FnMut(&mut EcsWorld, i32),
) {
    let FloatRowCtx {
        id_source,
        target,
        label,
        min,
        max,
        clip_start,
        clip_end,
        current_frame,
        base_value,
        track,
        button_w,
    } = ctx;
    let seeded;
    let track: &[crate::ecs::types::Keyframe] = if track.is_empty() {
        seeded = super::easing_editor::seed_start(world, &target, clip_start, base_value);
        &seeded
    } else {
        track
    };
    let segment = resolve_segment(track, current_frame, base_value);
    let outcome = property_row(
        ui,
        id_source,
        label,
        segment,
        min,
        max,
        button_w,
        !track.is_empty(),
    );
    if outcome.label_clicked {
        super::easing_editor::toggle(target.clone(), label);
    }

    if let Some(v) = outcome.start_value {
        let (e, p) = engine_of(track, segment.start_frame);
        set_kf(world, segment.start_frame, v, e, p);
    }
    if let Some(v) = outcome.end_value {
        let end_frame = if track.len() < 2 && clip_end > segment.start_frame {
            clip_end
        } else {
            segment.end_frame
        };
        let (e, p) = engine_of(track, end_frame);
        set_kf(world, end_frame, v, e, p);
    }

    let boundaries = super::segment::boundary_frames(track);
    let t_outcome = keyframe_track(
        ui,
        id_source,
        &boundaries,
        clip_start,
        clip_end,
        current_frame,
        segment.start_frame,
        segment.end_frame,
        |f| track.iter().any(|k| k.frame == f),
    );
    if let Some(f) = t_outcome.add_point {
        super::easing_editor::edit_track(world, &target, |t| {
            let _ = t.insert_point(f);
        });
    }
    if let Some(f) = t_outcome.remove_point {
        remove_kf(world, f);
    }
    if let Some(commit) = t_outcome.drag_committed {
        super::easing_editor::edit_track(world, &target, |t| commit.apply(t));
    }
    if t_outcome.equalize {
        super::easing_editor::edit_track(world, &target, |t| {
            let _ = t.equalize();
        });
    }
}

pub(super) struct ColorRowCtx<'a, S: std::hash::Hash + Copy + std::fmt::Debug> {
    pub id_source: S,
    pub object_id: usize,
    pub keys: [&'static str; 4],
    pub label: &'a str,
    pub clip_start: i32,
    pub clip_end: i32,
    pub current_frame: i32,
    pub base_value: [f32; 4],
    pub track: [Vec<crate::ecs::types::Keyframe>; 4],
    pub button_w: f32,
}

pub(super) fn color_row_ctx<S: std::hash::Hash + Copy + std::fmt::Debug>(
    ui: &mut egui::Ui,
    world: &mut EcsWorld,
    ctx: ColorRowCtx<S>,
) {
    let ColorRowCtx {
        id_source,
        object_id,
        keys,
        label,
        clip_start,
        clip_end,
        current_frame,
        base_value,
        track,
        button_w,
    } = ctx;

    let mut track = track;
    for i in 0..4 {
        if track[i].is_empty() {
            let target = super::easing_editor::TrackTarget::Object {
                object_id,
                key: keys[i].to_string(),
            };
            track[i] = super::easing_editor::seed_start(world, &target, clip_start, base_value[i]);
        }
    }
    let object_target = |i: usize| super::easing_editor::TrackTarget::Object {
        object_id,
        key: keys[i].to_string(),
    };

    let segments: [super::segment::Segment; 4] =
        std::array::from_fn(|i| resolve_segment(&track[i], current_frame, base_value[i]));
    let start_color = [
        segments[0].start_value,
        segments[1].start_value,
        segments[2].start_value,
        segments[3].start_value,
    ];
    let end_color = [
        segments[0].end_value,
        segments[1].end_value,
        segments[2].end_value,
        segments[3].end_value,
    ];

    let outcome = color_row(
        ui,
        id_source,
        label,
        start_color,
        end_color,
        button_w,
        track.iter().any(|channel| !channel.is_empty()),
    );
    if outcome.label_clicked {
        super::easing_editor::toggle(
            super::easing_editor::TrackTarget::Object {
                object_id,
                key: keys[0].to_string(),
            },
            label,
        );
    }

    if let Some(c) = outcome.start_color {
        for i in 0..4 {
            let (e, p) = engine_of(&track[i], segments[i].start_frame);
            world.set_keyframe(object_id, keys[i], segments[i].start_frame, c[i], e, p);
        }
    }
    if let Some(c) = outcome.end_color {
        for i in 0..4 {
            let end_frame = if track[i].len() < 2 && clip_end > segments[i].start_frame {
                clip_end
            } else {
                segments[i].end_frame
            };
            let (e, p) = engine_of(&track[i], end_frame);
            world.set_keyframe(object_id, keys[i], end_frame, c[i], e, p);
        }
    }

    let mut boundary_set = std::collections::BTreeSet::new();
    for channel in &track {
        for f in super::segment::boundary_frames(channel) {
            boundary_set.insert(f);
        }
    }
    let boundaries: Vec<i32> = boundary_set.into_iter().collect();

    let marker_color = |f: i32| -> Option<egui::Color32> {
        let ch = |i: usize| -> f32 {
            track[i]
                .iter()
                .find(|k| k.frame == f)
                .map(|k| k.value)
                .unwrap_or(base_value[i])
        };
        Some(egui::Color32::from_rgba_unmultiplied(
            (ch(0) * 255.0).round() as u8,
            (ch(1) * 255.0).round() as u8,
            (ch(2) * 255.0).round() as u8,
            (ch(3) * 255.0).round() as u8,
        ))
    };

    let t_outcome = keyframe_track_colored(
        ui,
        id_source,
        &boundaries,
        clip_start,
        clip_end,
        current_frame,
        segments[0].start_frame,
        segments[0].end_frame,
        marker_color,
        |f| {
            track
                .iter()
                .any(|channel| channel.iter().any(|k| k.frame == f))
        },
    );

    if let Some(f) = t_outcome.add_point {
        for i in 0..4 {
            super::easing_editor::edit_track(world, &object_target(i), |t| {
                let _ = t.insert_point(f);
            });
        }
    }
    if let Some(f) = t_outcome.remove_point {
        for key in keys {
            world.remove_keyframe(object_id, key, f);
        }
    }
    if let Some(commit) = t_outcome.drag_committed {
        for i in 0..4 {
            super::easing_editor::edit_track(world, &object_target(i), |t| commit.apply(t));
        }
    }
    if t_outcome.equalize {
        for i in 0..4 {
            super::easing_editor::edit_track(world, &object_target(i), |t| {
                let _ = t.equalize();
            });
        }
    }
}

pub(super) fn engine_of(track: &[crate::ecs::types::Keyframe], frame: i32) -> (String, Vec<u8>) {
    track
        .iter()
        .find(|k| k.frame == frame)
        .or_else(|| track.last())
        .map(|k| (k.engine_id.clone(), k.engine_payload.clone()))
        .unwrap_or(("neoutl-easing-standard".into(), Vec::new()))
}

pub fn transform_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut transform) = world.get_transform(id) else {
        return;
    };
    let (clip_start, clip_end) = clip_bounds(world, id);
    let current_frame = world.current_frame();
    let button_w = super::row::button_column_width(
        ui,
        TRANSFORM_SCHEMA
            .iter()
            .filter(|s| s.kind == ParamKind::Float)
            .map(|s| effect_param_label(s.label)),
    );
    for schema in TRANSFORM_SCHEMA {
        let Some(value) = transform.get_param(schema.key) else {
            continue;
        };
        match schema.kind {
            ParamKind::Bool => {
                tui(ui, ui.id().with((id, "transform_bool", schema.key)))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.label(effect_param_label(schema.label));
                        });
                        tui.ui(|ui| {
                            let mut b = value > 0.5;
                            if ui.add(Checkbox::new(&mut b, "")).changed() {
                                transform.set_param(schema.key, if b { 1.0 } else { 0.0 });
                                world.set_transform(id, transform);
                            }
                        });
                    });
            }
            ParamKind::Float => {
                let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                let track = world.get_keyframes(id, schema.key);
                float_row(
                    ui,
                    world,
                    FloatRowCtx {
                        id_source: (id, "transform", schema.key),
                        target: super::easing_editor::TrackTarget::Object {
                            object_id: id,
                            key: schema.key.to_string(),
                        },
                        label: schema.label,
                        min,
                        max,
                        clip_start,
                        clip_end,
                        current_frame,
                        base_value: value,
                        track: &track,
                        button_w,
                    },
                    |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
                    |w, f| w.remove_keyframe(id, schema.key, f),
                );
            }
            _ => {}
        }
    }
}

pub fn text_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut content) = world.get_text(id) else {
        return;
    };
    let (clip_start, clip_end) = clip_bounds(world, id);
    let current_frame = world.current_frame();
    let button_w = super::row::button_column_width(
        ui,
        TEXT_SCHEMA
            .iter()
            .filter(|s| s.kind == ParamKind::Float)
            .map(|s| effect_param_label(s.label)),
    );
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("テキスト"));
    ui.label(effect_param_label("フォント候補"));
    let mut stack = content.font_family_stack.clone();
    let mut remove_at: Option<usize> = None;
    let mut updated: Option<Vec<String>> = None;
    for row in 0..stack.len() {
        tui(ui, ui.id().with((id, "font_stack_row_tui", row)))
            .style(row_style(6.0))
            .show(|tui| {
                tui.style(crate::ui::ui_ext::grow_style()).ui(|ui| {
                    let mut family = stack[row].clone();
                    if let Some(new_family) = crate::ui::font_stack::font_stack_row(
                        ui,
                        (id, "font_stack", row),
                        &mut family,
                    ) {
                        stack[row] = new_family;
                        updated = Some(stack.clone());
                    }
                });
                tui.ui(|ui| {
                    ui.add_enabled_ui(stack.len() > 1, |ui| {
                        if ui.small_button("✕").clicked() {
                            remove_at = Some(row);
                        }
                    });
                });
            });
    }
    if let Some(new_stack) = updated {
        world.set_text_font_stack(id, new_stack);
    }
    if let Some(row) = remove_at {
        stack.remove(row);
        world.set_text_font_stack(id, stack.clone());
    }
    if ui.small_button("+").clicked() {
        stack.push(String::new());
        world.set_text_font_stack(id, stack);
    }
    for schema in TEXT_SCHEMA {
        if !is_visible(schema, |k| content.get_param(k).unwrap_or(0.0)) {
            continue;
        }
        match schema.kind {
            ParamKind::Text if schema.key == "text" => {
                ui.label(effect_param_label(schema.label));
                let width = ui.available_width();
                if ui
                    .add_sized([width, 80.0], TextArea::new(&mut content.text).rows(4))
                    .changed()
                {
                    world.set_text(id, content.text.clone(), content.font_size);
                }
            }
            ParamKind::Text if schema.key == "font_family" => {}
            ParamKind::Text => {}
            ParamKind::Bool => {
                tui(ui, ui.id().with((id, "text_bool", schema.key)))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.label(effect_param_label(schema.label));
                        });
                        tui.ui(|ui| {
                            let mut b = content.get_param(schema.key).unwrap_or(0.0) > 0.5;
                            if ui.add(Checkbox::new(&mut b, "")).changed() {
                                world.set_text_param(id, schema.key, if b { 1.0 } else { 0.0 });
                            }
                        });
                    });
            }
            ParamKind::Enum => {
                tui(ui, ui.id().with((id, "text_enum", schema.key)))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.label(effect_param_label(schema.label));
                        });
                        tui.ui(|ui| {
                            let mut idx =
                                content.get_param(schema.key).unwrap_or(0.0).round() as usize;
                            let resp = ui.add(
                                Select::new((ui.id(), "text", schema.key), &mut idx).options(
                                    schema
                                        .enum_options
                                        .iter()
                                        .enumerate()
                                        .map(|(i, o)| (i, effect_param_label(o).to_string())),
                                ),
                            );
                            if resp.changed() {
                                world.set_text_param(id, schema.key, idx as f32);
                            }
                        });
                    });
            }
            ParamKind::Float => {
                let value = content.get_param(schema.key).unwrap_or(0.0);
                let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                let track = world.get_keyframes(id, schema.key);
                float_row(
                    ui,
                    world,
                    FloatRowCtx {
                        id_source: (id, "text", schema.key),
                        target: super::easing_editor::TrackTarget::Object {
                            object_id: id,
                            key: schema.key.to_string(),
                        },
                        label: schema.label,
                        min,
                        max,
                        clip_start,
                        clip_end,
                        current_frame,
                        base_value: value,
                        track: &track,
                        button_w,
                    },
                    |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
                    |w, f| w.remove_keyframe(id, schema.key, f),
                );
            }
            _ => {}
        }
    }
}

pub fn shape_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(shape) = world.get_shape(id) else {
        return;
    };
    let (clip_start, clip_end) = clip_bounds(world, id);
    let current_frame = world.current_frame();
    let button_w = super::row::button_column_width(
        ui,
        SHAPE_SCHEMA
            .iter()
            .filter(|s| s.kind == ParamKind::Float)
            .map(|s| effect_param_label(s.label))
            .chain(
                SHAPE_COLOR_SCHEMA
                    .iter()
                    .map(|s| effect_param_label(s.label)),
            ),
    );
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("図形"));
    for schema in SHAPE_SCHEMA {
        let value = shape.get_param(schema.key).unwrap_or(0.0);
        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
        let track = world.get_keyframes(id, schema.key);
        float_row(
            ui,
            world,
            FloatRowCtx {
                id_source: (id, "shape", schema.key),
                target: super::easing_editor::TrackTarget::Object {
                    object_id: id,
                    key: schema.key.to_string(),
                },
                label: schema.label,
                min,
                max,
                clip_start,
                clip_end,
                current_frame,
                base_value: value,
                track: &track,
                button_w,
            },
            |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
            |w, f| w.remove_keyframe(id, schema.key, f),
        );
    }
    for ColorParamSchema { keys, label } in SHAPE_COLOR_SCHEMA {
        let base_value: [f32; 4] = std::array::from_fn(|i| shape.get_param(keys[i]).unwrap_or(0.0));
        let track: [Vec<crate::ecs::types::Keyframe>; 4] =
            std::array::from_fn(|i| world.get_keyframes(id, keys[i]));
        color_row_ctx(
            ui,
            world,
            ColorRowCtx {
                id_source: (id, "shape_color", *label),
                object_id: id,
                keys: *keys,
                label,
                clip_start,
                clip_end,
                current_frame,
                base_value,
                track,
                button_w,
            },
        );
    }
}

pub fn audio_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut audio) = world.get_audio_params(id) else {
        return;
    };
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("オーディオ"));
    for schema in AUDIO_SCHEMA {
        if !is_visible(schema, |k| audio.get_param(k).unwrap_or(0.0)) {
            continue;
        }
        tui(ui, ui.id().with((id, "audio_param", schema.key)))
            .style(row_style(6.0))
            .show(|tui| {
                tui.ui(|ui| {
                    ui.label(effect_param_label(schema.label));
                });
                tui.ui(|ui| match schema.kind {
                    ParamKind::Bool => {
                        let mut b = audio.get_param(schema.key).unwrap_or(0.0) > 0.5;
                        if ui.add(Checkbox::new(&mut b, "")).changed() {
                            audio.set_param(schema.key, if b { 1.0 } else { 0.0 });
                            world.set_audio_params(id, audio.volume, audio.pan, audio.mute);
                        }
                    }
                    ParamKind::Float => {
                        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                        let mut value = audio.get_param(schema.key).unwrap_or(0.0);
                        if ui
                            .add(
                                Slider::new(&mut value, min..=max)
                                    .step(((max - min).max(0.001) / 1000.0) as f64),
                            )
                            .changed()
                        {
                            audio.set_param(schema.key, value);
                            world.set_audio_params(id, audio.volume, audio.pan, audio.mute);
                        }
                    }
                    _ => {}
                });
            });
    }
}

const GROUP_CONTROL_STRUCTURAL_KEYS: &[&str] =
    &["layer_count_down", "layer_count_up", "camera_target_layer"];

pub fn group_control_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut gc) = world.get_group_control(id) else {
        return;
    };
    let (clip_start, clip_end) = clip_bounds(world, id);
    let current_frame = world.current_frame();
    let button_w = super::row::button_column_width(
        ui,
        GROUP_CONTROL_SCHEMA
            .iter()
            .filter(|s| {
                s.kind == ParamKind::Float && !GROUP_CONTROL_STRUCTURAL_KEYS.contains(&s.key)
            })
            .map(|s| effect_param_label(s.label)),
    );
    ui.separator();
    ui.colored_label(
        egui::Color32::from_rgb(0x8a, 0xab, 0xff),
        t!("グループ制御"),
    );
    for schema in GROUP_CONTROL_SCHEMA {
        if !is_visible(schema, |key| gc.get_param(key).unwrap_or(0.0)) {
            continue;
        }
        match schema.kind {
            ParamKind::Bool => {
                tui(ui, ui.id().with((id, "group_control_bool", schema.key)))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.label(effect_param_label(schema.label));
                        });
                        tui.ui(|ui| {
                            let mut b = gc.get_param(schema.key).unwrap_or(0.0) > 0.5;
                            if ui.add(Checkbox::new(&mut b, "")).changed() {
                                gc.set_param(schema.key, if b { 1.0 } else { 0.0 });
                                world.set_group_control(id, gc);
                            }
                        });
                    });
            }
            ParamKind::Float if GROUP_CONTROL_STRUCTURAL_KEYS.contains(&schema.key) => {
                tui(
                    ui,
                    ui.id().with((id, "group_control_structural", schema.key)),
                )
                .style(row_style(6.0))
                .show(|tui| {
                    tui.ui(|ui| {
                        ui.label(effect_param_label(schema.label));
                    });
                    tui.ui(|ui| {
                        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                        let mut value = gc.get_param(schema.key).unwrap_or(0.0);
                        if ui
                            .add(
                                Slider::new(&mut value, min..=max)
                                    .step(((max - min).max(0.001) / 1000.0) as f64),
                            )
                            .changed()
                        {
                            gc.set_param(schema.key, value.round());
                            world.set_group_control(id, gc);
                        }
                    });
                });
            }
            ParamKind::Float => {
                let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                let value = gc.get_param(schema.key).unwrap_or(0.0);
                let track = world.get_keyframes(id, schema.key);
                float_row(
                    ui,
                    world,
                    FloatRowCtx {
                        id_source: (id, "group_control", schema.key),
                        target: super::easing_editor::TrackTarget::Object {
                            object_id: id,
                            key: schema.key.to_string(),
                        },
                        label: schema.label,
                        min,
                        max,
                        clip_start,
                        clip_end,
                        current_frame,
                        base_value: value,
                        track: &track,
                        button_w,
                    },
                    |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
                    |w, f| w.remove_keyframe(id, schema.key, f),
                );
            }
            ParamKind::Enum => {
                tui(ui, ui.id().with((id, "group_control_enum", schema.key)))
                    .style(row_style(6.0))
                    .show(|tui| {
                        tui.ui(|ui| {
                            ui.label(effect_param_label(schema.label));
                        });
                        tui.ui(|ui| {
                            let mut current =
                                gc.get_param(schema.key).unwrap_or(0.0).round() as usize;
                            let resp = ui.add(
                                Select::new((id, schema.key), &mut current).options(
                                    schema
                                        .enum_options
                                        .iter()
                                        .enumerate()
                                        .map(|(i, opt)| (i, *opt)),
                                ),
                            );
                            if resp.changed() {
                                gc.set_param(schema.key, current as f32);
                                world.set_group_control(id, gc);
                            }
                        });
                    });
            }
            _ => {}
        }
    }
}

const BLEND_MODE_OPTIONS: [(crate::ecs::components::BlendMode, &str); 9] = [
    (crate::ecs::components::BlendMode::Normal, "通常"),
    (crate::ecs::components::BlendMode::Add, "加算"),
    (crate::ecs::components::BlendMode::Multiply, "乗算"),
    (crate::ecs::components::BlendMode::Screen, "スクリーン"),
    (crate::ecs::components::BlendMode::Overlay, "オーバーレイ"),
    (crate::ecs::components::BlendMode::Darken, "比較(暗)"),
    (crate::ecs::components::BlendMode::Lighten, "比較(明)"),
    (crate::ecs::components::BlendMode::Difference, "差の絶対値"),
    (crate::ecs::components::BlendMode::Exclusion, "除外"),
];

pub fn compositing_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("合成"));

    tui(ui, ui.id().with((id, "compositing_blend_mode")))
        .style(row_style(6.0))
        .show(|tui| {
            tui.ui(|ui| {
                ui.label(t!("ブレンドモード"));
            });
            tui.ui(|ui| {
                let current_mode = world.blend_mode_of(id);
                let mut current = BLEND_MODE_OPTIONS
                    .iter()
                    .position(|(m, _)| *m == current_mode)
                    .unwrap_or(0);
                let resp = ui.add(
                    Select::new((id, "blend_mode"), &mut current).options(
                        BLEND_MODE_OPTIONS
                            .iter()
                            .enumerate()
                            .map(|(i, (_, label))| (i, *label)),
                    ),
                );
                if resp.changed() {
                    if let Some((mode, _)) = BLEND_MODE_OPTIONS.get(current) {
                        world.set_blend_mode(id, *mode);
                    }
                }
            });
        });
}

pub fn mask_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("マスク"));
    let indices: Vec<usize> = world
        .effect_stack_of(id)
        .iter()
        .enumerate()
        .filter(|(_, e)| e.effect_id == "mask_shape")
        .map(|(i, _)| i)
        .collect();
    for &i in &indices {
        ui.push_id(i, |ui| {
            tui(ui, ui.id().with("mask_header_row"))
                .style(row_style(6.0))
                .show(|tui| {
                    tui.ui(|ui| {
                        ui.label(format!("#{i}"));
                    });
                    tui.ui(|ui| {
                        if ui.small_button(t!("削除")).clicked() {
                            world.remove_effect(id, i);
                        }
                    });
                });
            tui(ui, ui.id().with("mask_radius_row"))
                .style(row_style(6.0))
                .show(|tui| {
                    tui.ui(|ui| {
                        ui.label(t!("半径/サイズ"));
                    });
                    tui.ui(|ui| {
                        let mut radius = world.effect_param_f32(id, i, "radius").unwrap_or(0.4);
                        if ui.add(Slider::new(&mut radius, 0.0..=1.0)).changed() {
                            world.set_effect_param(id, i, "radius", radius);
                        }
                    });
                });
            tui(ui, ui.id().with("mask_feather_row"))
                .style(row_style(6.0))
                .show(|tui| {
                    tui.ui(|ui| {
                        ui.label(t!("フェザー"));
                    });
                    tui.ui(|ui| {
                        let mut feather = world.effect_param_f32(id, i, "feather").unwrap_or(0.0);
                        if ui.add(Slider::new(&mut feather, 0.0..=0.5)).changed() {
                            world.set_effect_param(id, i, "feather", feather);
                        }
                    });
                    tui.ui(|ui| {
                        let mut invert = world.effect_param_bool(id, i, "invert");
                        if ui.add(Checkbox::new(&mut invert, t!("反転"))).changed() {
                            world.set_effect_param_bool(id, i, "invert", invert);
                        }
                    });
                });
        });
    }
    if ui.small_button(t!("マスクを追加")).clicked() {
        world.add_effect(id, "mask_shape");
    }
}

pub fn time_remap_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    ui.separator();
    ui.colored_label(
        egui::Color32::from_rgb(0x8a, 0xab, 0xff),
        t!("タイムリマップ"),
    );
    let mut remap = world.time_remap_of(id);
    let mut changed = false;

    let mut freeze_enabled = remap.freeze_frame.is_some();
    tui(ui, ui.id().with((id, "time_remap_freeze")))
        .style(row_style(6.0))
        .show(|tui| {
            tui.ui(|ui| {
                if ui
                    .add(Checkbox::new(&mut freeze_enabled, t!("フリーズフレーム")))
                    .changed()
                {
                    remap.freeze_frame = if freeze_enabled { Some(0) } else { None };
                    changed = true;
                }
            });
            tui.style(crate::ui::ui_ext::grow_style()).ui(|ui| {
                if let Some(frame) = remap.freeze_frame.as_mut() {
                    let mut v = *frame as f32;
                    if ui.add(Slider::new(&mut v, 0.0..=100000.0)).changed() {
                        *frame = v.round() as i32;
                        changed = true;
                    }
                }
            });
        });

    let mut remove_idx: Option<usize> = None;
    let mut move_request: Option<(usize, i32)> = None;
    for (i, k) in remap.keyframes.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            tui(ui, ui.id().with("time_remap_keyframe_row"))
                .style(row_style(6.0))
                .show(|tui| {
                    tui.ui(|ui| {
                        ui.label(t!("入力フレーム"));
                    });
                    tui.ui(|ui| {
                        let mut kf = k.frame as f32;
                        if ui.add(Slider::new(&mut kf, 0.0..=100000.0)).changed() {
                            move_request = Some((i, kf.round() as i32));
                        }
                    });
                    tui.ui(|ui| {
                        ui.label(t!("出力フレーム"));
                    });
                    tui.ui(|ui| {
                        let mut vf = k.value;
                        if ui.add(Slider::new(&mut vf, 0.0..=100000.0)).changed() {
                            k.value = vf;
                            k.edit_seq = crate::ecs::types::next_edit_seq();
                            changed = true;
                        }
                    });
                    tui.ui(|ui| {
                        if ui.small_button(t!("削除")).clicked() {
                            remove_idx = Some(i);
                        }
                    });
                });
        });
    }
    if let Some((i, frame)) = move_request {
        changed |= remap.move_key(i, frame).is_some();
    }
    if let Some(i) = remove_idx {
        remap.keyframes.remove(i);
        changed = true;
    }
    if ui.small_button(t!("キーを追加")).clicked() {
        let last = remap
            .keyframes
            .last()
            .map(|k| (k.frame, k.value))
            .unwrap_or((0, 0.0));
        remap.keyframes.push(crate::ecs::types::Keyframe::new(
            last.0 + 30,
            last.1 + 30.0,
            "neoutl-easing-standard".to_string(),
            Vec::new(),
        ));
        remap.keyframes.sort_by_key(|k| k.frame);
        changed = true;
    }
    if changed {
        world.set_time_remap(id, remap);
    }
}

pub fn clip_target_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let mut ct = world.get_clip_target(id);
    ui.separator();
    ui.colored_label(
        egui::Color32::from_rgb(0xe0, 0x8a, 0x50),
        t!("クリッピング制御"),
    );
    for schema in CLIP_TARGET_SCHEMA {
        if schema.key != CLIP_TARGET_ENABLED_KEY && !ct.enabled {
            continue;
        }
        if !is_visible(schema, |key| ct.get_param(key).unwrap_or(0.0)) {
            continue;
        }
        tui(ui, ui.id().with((id, "clip_target_param", schema.key)))
            .style(row_style(6.0))
            .show(|tui| {
                tui.ui(|ui| {
                    ui.label(effect_param_label(schema.label));
                });
                tui.ui(|ui| match schema.kind {
                    ParamKind::Bool => {
                        let mut b = ct.get_param(schema.key).unwrap_or(0.0) > 0.5;
                        if ui.add(Checkbox::new(&mut b, "")).changed() {
                            ct.set_param(schema.key, if b { 1.0 } else { 0.0 });
                            world.set_clip_target(id, ct);
                        }
                    }
                    ParamKind::Float => {
                        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                        let mut value = ct.get_param(schema.key).unwrap_or(0.0);
                        if ui
                            .add(
                                Slider::new(&mut value, min..=max)
                                    .step(((max - min).max(0.001) / 1000.0) as f64),
                            )
                            .changed()
                        {
                            ct.set_param(schema.key, value.round());
                            world.set_clip_target(id, ct);
                        }
                    }
                    ParamKind::Enum => {
                        let mut current = ct.get_param(schema.key).unwrap_or(0.0).round() as usize;
                        let resp = ui.add(
                            Select::new((id, schema.key), &mut current).options(
                                schema
                                    .enum_options
                                    .iter()
                                    .enumerate()
                                    .map(|(i, opt)| (i, *opt)),
                            ),
                        );
                        if resp.changed() {
                            ct.set_param(schema.key, current as f32);
                            world.set_clip_target(id, ct);
                        }
                    }
                    _ => {}
                });
            });
    }
}

pub(super) fn clip_bounds(world: &EcsWorld, id: usize) -> (i32, i32) {
    world
        .get_timeline_objects()
        .into_iter()
        .find(|o| o.id as usize == id)
        .map(|o| (o.start_frame, o.end_frame))
        .unwrap_or((0, 0))
}
