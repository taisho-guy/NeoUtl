use super::row::property_row;
use super::segment::resolve_segment;
use super::track::keyframe_track;
use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::object_schema::{
    AUDIO_SCHEMA, SHAPE_SCHEMA, TEXT_SCHEMA, TRANSFORM_SCHEMA, is_visible, resolve_range,
};
use neoutl_shared_abi::ParamKind;

/// Float/Color系パラメータ1行の共通描画。呼び出し側は`get`/`set`/`track`/`set_kf`/
/// `remove_kf`をobject種別ごとのアクセサとして渡し、本関数は書き込み先フレーム確定
/// （resolve_segment）とUI描画のみを担う。
#[allow(clippy::too_many_arguments)]
pub(super) fn float_row(
    ui: &mut egui::Ui,
    world: &mut EcsWorld,
    id_source: impl std::hash::Hash + Copy + std::fmt::Debug,
    target: super::easing_editor::TrackTarget,
    label: &str,
    min: f32,
    max: f32,
    clip_start: i32,
    clip_end: i32,
    current_frame: i32,
    base_value: f32,
    track: &[crate::ecs::types::Keyframe],
    mut set_kf: impl FnMut(&mut EcsWorld, i32, f32, String, Vec<u8>),
    mut remove_kf: impl FnMut(&mut EcsWorld, i32),
) {
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
                    ui.label(schema.label);
                    let mut b = value > 0.5;
                    if ui.checkbox(&mut b, "").changed() {
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
                    (id, "transform", schema.key),
                    super::easing_editor::TrackTarget::Object {
                        object_id: id,
                        key: schema.key.to_string(),
                    },
                    schema.label,
                    min,
                    max,
                    clip_start,
                    clip_end,
                    current_frame,
                    value,
                    &track,
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
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "テキスト");
    for schema in TEXT_SCHEMA {
        match schema.kind {
            ParamKind::Text => {
                ui.horizontal(|ui| {
                    ui.label(schema.label);
                    if ui.text_edit_multiline(&mut content.text).changed() {
                        world.set_text(id, content.text.clone(), content.font_size);
                    }
                });
            }
            ParamKind::Float => {
                let value = content.get_param(schema.key).unwrap_or(0.0);
                let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                let track = world.get_keyframes(id, schema.key);
                float_row(
                    ui,
                    world,
                    (id, "text", schema.key),
                    super::easing_editor::TrackTarget::Object {
                        object_id: id,
                        key: schema.key.to_string(),
                    },
                    schema.label,
                    min,
                    max,
                    clip_start,
                    clip_end,
                    current_frame,
                    value,
                    &track,
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
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "図形");
    for schema in SHAPE_SCHEMA {
        let value = shape.get_param(schema.key).unwrap_or(0.0);
        let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
        let track = world.get_keyframes(id, schema.key);
        float_row(
            ui,
            world,
            (id, "shape", schema.key),
            super::easing_editor::TrackTarget::Object {
                object_id: id,
                key: schema.key.to_string(),
            },
            schema.label,
            min,
            max,
            clip_start,
            clip_end,
            current_frame,
            value,
            &track,
            |w, f, v, e, p| w.set_keyframe(id, schema.key, f, v, e, p),
            |w, f| w.remove_keyframe(id, schema.key, f),
        );
    }
}

/// オーディオ("volume"/"pan")はキーフレーム非対応のためPropertyRowではなく
/// 単一DragValueのまま据え置く（旧properties_panel.rsの方針を継承）。
pub fn audio_section(ui: &mut egui::Ui, world: &mut EcsWorld, id: usize) {
    let Some(mut audio) = world.get_audio_params(id) else {
        return;
    };
    ui.separator();
    ui.colored_label(egui::Color32::from_rgb(0x8a, 0xab, 0xff), "オーディオ");
    for schema in AUDIO_SCHEMA {
        if !is_visible(schema, |k| audio.get_param(k).unwrap_or(0.0)) {
            continue;
        }
        ui.horizontal(|ui| {
            ui.label(schema.label);
            match schema.kind {
                ParamKind::Bool => {
                    let mut b = audio.get_param(schema.key).unwrap_or(0.0) > 0.5;
                    if ui.checkbox(&mut b, "").changed() {
                        audio.set_param(schema.key, if b { 1.0 } else { 0.0 });
                        world.set_audio_params(id, audio.volume, audio.pan, audio.mute);
                    }
                }
                ParamKind::Float => {
                    let (min, max) = resolve_range(schema.range, 1920.0, 1080.0);
                    let mut value = audio.get_param(schema.key).unwrap_or(0.0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut value)
                                .range(min..=max)
                                .speed((max - min).max(0.001) / 1000.0),
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

pub(super) fn clip_bounds(world: &EcsWorld, id: usize) -> (i32, i32) {
    world
        .get_timeline_objects()
        .into_iter()
        .find(|o| o.id as usize == id)
        .map(|o| (o.start_frame, o.end_frame))
        .unwrap_or((0, 0))
}
