use super::row::property_row;
use super::segment::resolve_segment;
use super::track::keyframe_track;
use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::object_schema::{
    AUDIO_SCHEMA, CLIP_TARGET_ENABLED_KEY, CLIP_TARGET_SCHEMA, GROUP_CONTROL_SCHEMA, SHAPE_SCHEMA,
    TEXT_SCHEMA, TRANSFORM_SCHEMA, is_visible, resolve_range,
};
use crate::localization::effect_param_label;
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
    } = ctx;
    let segment = resolve_segment(track, clip_start, clip_end, current_frame, base_value);
    let outcome = property_row(ui, id_source, label, segment, min, max);
    if outcome.label_clicked {
        super::easing_editor::toggle(target, label);
    }

    if let Some(v) = outcome.start_value {
        let (e, p) = engine_of(track, segment.start_frame);
        set_kf(world, segment.start_frame, v, e, p);
    }
    if let Some(v) = outcome.end_value {
        let (e, p) = engine_of(track, segment.end_frame);
        set_kf(world, segment.end_frame, v, e, p);
    }

    let boundaries = super::segment::boundary_frames(track, clip_start, clip_end);
    let t_outcome = keyframe_track(
        ui,
        id_source,
        &boundaries,
        clip_start,
        clip_end,
        current_frame,
        segment.start_frame,
        segment.end_frame,
    );
    if let Some(f) = t_outcome.add_point {
        let (e, p) = engine_of(track, f);
        set_kf(world, f, base_value, e, p);
    }
    if let Some(f) = t_outcome.remove_point {
        remove_kf(world, f);
    }
    if let Some((from, to)) = t_outcome.drag_committed {
        if let Some(k) = track.iter().find(|k| k.frame == from) {
            let (e, p, v) = (k.engine_id.clone(), k.engine_payload.clone(), k.value);
            remove_kf(world, from);
            set_kf(world, to, v, e, p);
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
    for schema in TRANSFORM_SCHEMA {
        let Some(value) = transform.get_param(schema.key) else {
            continue;
        };
        match schema.kind {
            ParamKind::Bool => {
                ui.horizontal(|ui| {
                    ui.label(effect_param_label(schema.label));
                    let mut b = value > 0.5;
                    if ui.add(Checkbox::new(&mut b, "")).changed() {
                        transform.set_param(schema.key, if b { 1.0 } else { 0.0 });
                        world.set_transform(id, transform);
                    }
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
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), t!("テキスト"));
    for schema in TEXT_SCHEMA {
        match schema.kind {
            ParamKind::Text => {
                ui.label(effect_param_label(schema.label));
                let width = ui.available_width();
                if ui
                    .add_sized([width, 80.0], TextArea::new(&mut content.text).rows(4))
                    .changed()
                {
                    world.set_text(id, content.text.clone(), content.font_size);
                }
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
            },
            |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
            |w, f| w.remove_keyframe(id, schema.key, f),
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
        ui.horizontal(|ui| {
            ui.label(effect_param_label(schema.label));
            match schema.kind {
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
            }
        });
    }
}

pub fn group_control_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut gc) = world.get_group_control(id) else {
        return;
    };
    ui.separator();
    ui.colored_label(
        egui::Color32::from_rgb(0x8a, 0xab, 0xff),
        t!("グループ制御"),
    );
    for schema in GROUP_CONTROL_SCHEMA {
        ui.horizontal(|ui| {
            ui.label(effect_param_label(schema.label));
            match schema.kind {
                ParamKind::Bool => {
                    let mut b = gc.get_param(schema.key).unwrap_or(0.0) > 0.5;
                    if ui.add(Checkbox::new(&mut b, "")).changed() {
                        gc.set_param(schema.key, if b { 1.0 } else { 0.0 });
                        world.set_group_control(id, gc);
                    }
                }
                ParamKind::Float => {
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
                }
                _ => {}
            }
        });
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
        ui.horizontal(|ui| {
            ui.label(effect_param_label(schema.label));
            match schema.kind {
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
            }
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
