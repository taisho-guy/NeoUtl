// src/ui/properties.rs
use crate::app_state::{self, SharedAppState};
use crate::ecs::{
    EcsWorld, components::ShapeParams, effects::EFFECT_REGISTRY, effects::EffectMetadata,
    effects::ParamKind, effects::find_effect,
};
use crate::{CatalogRow, EffectRow, ParamRow, PropertiesWindow};
use slint::{ComponentHandle, ModelRc, VecModel};

pub fn setup(props: &PropertiesWindow, state: SharedAppState) {
    let catalog: Vec<CatalogRow> = EFFECT_REGISTRY
        .iter()
        .map(|m| CatalogRow {
            id: m.id.into(),
            name: m.name.into(),
        })
        .collect();
    props.set_effect_catalog(ModelRc::new(VecModel::from(catalog)));

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_transform(move |x, y, z, sx, sy, rx, ry, rz, op| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_transform(
                id as usize,
                crate::ecs::transform::Transform {
                    x,
                    y,
                    z,
                    scale_x: sx,
                    scale_y: sy,
                    rot_x: rx,
                    rot_y: ry,
                    rot_z: rz,
                    opacity: op,
                },
            );
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_text(move |text, x, y, font_size| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::active_world(&state).lock().unwrap().set_text(
                id as usize,
                text.to_string(),
                x,
                y,
                font_size,
            );
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_shape(move |sides, extrude_depth, stroke_width, r, g, b, a| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let mut shape = world.get_shape(id as usize).unwrap_or_default();
            shape.sides = sides.max(3.0) as u32;
            shape.extrude_depth = extrude_depth.max(0.0);
            shape.stroke_width = stroke_width.max(0.0);
            shape.fill_color = [r, g, b, a];
            world.set_shape(id as usize, shape);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_audio(move |volume, pan, mute| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::active_world(&state)
                .lock()
                .unwrap()
                .set_audio_params(id as usize, volume, pan, mute);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_effect_enabled(move |index, enabled| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_enabled(id as usize, index as usize, enabled);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_remove_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.remove_effect(id as usize, index as usize);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            app_state::active_world(&state)
                .lock()
                .unwrap()
                .set_effect_param(id as usize, index as usize, key.as_str(), value);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_param_bool(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param_bool(id as usize, index as usize, key.as_str(), value);
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_add_effect(move |effect_id| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.add_effect(id as usize, effect_id.as_str());
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_move_effect(move |from, to| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 || from < 0 || to < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.reorder_effect(id as usize, from as usize, to as usize);
            refresh(&p, &world);
        });
    }
}

pub fn select_object(props: &PropertiesWindow, world: &EcsWorld, object_id: i32) {
    props.set_object_id(object_id);
    refresh(props, world);
}

fn refresh(props: &PropertiesWindow, world: &EcsWorld) {
    let id = props.get_object_id();
    if id < 0 {
        return;
    }
    let oid = id as usize;

    let project = world.get_project();
    props.set_stage_width(project.width as f32);
    props.set_stage_height(project.height as f32);

    if let Some(t) = world.get_transform(oid) {
        props.set_has_transform(true);
        props.set_tx(t.x);
        props.set_ty(t.y);
        props.set_tz(t.z);
        props.set_scale_x(t.scale_x);
        props.set_scale_y(t.scale_y);
        props.set_rot_x(t.rot_x);
        props.set_rot_y(t.rot_y);
        props.set_rot_z(t.rot_z);
        props.set_obj_opacity(t.opacity);
    } else {
        props.set_has_transform(false);
    }

    if let Some(text) = world.get_text(oid) {
        props.set_has_text(true);
        props.set_text_content(text.text.into());
        props.set_text_x(text.x);
        props.set_text_y(text.y);
        props.set_text_font_size(text.font_size);
    } else {
        props.set_has_text(false);
    }

    let shape: ShapeParams = world.get_shape(oid).unwrap_or_default();
    if world.get_shape(oid).is_some() {
        props.set_has_shape(true);
        props.set_shape_sides(shape.sides as f32);
        props.set_shape_extrude_depth(shape.extrude_depth);
        props.set_shape_stroke_width(shape.stroke_width);
        props.set_shape_r(shape.fill_color[0]);
        props.set_shape_g(shape.fill_color[1]);
        props.set_shape_b(shape.fill_color[2]);
        props.set_shape_a(shape.fill_color[3]);
    } else {
        props.set_has_shape(false);
    }

    if let Some(audio) = world.get_audio_params(oid) {
        props.set_has_audio(true);
        props.set_volume(audio.volume);
        props.set_pan(audio.pan);
        props.set_mute(audio.mute);
    } else {
        props.set_has_audio(false);
    }

    let instances = world.get_effects(oid);
    let rows: Vec<EffectRow> = instances
        .iter()
        .enumerate()
        .map(|(i, e)| EffectRow {
            index: i as i32,
            name: find_effect(&e.effect_id)
                .map(|m: &EffectMetadata| m.name)
                .unwrap_or(e.effect_id.as_str())
                .into(),
            enabled: e.enabled,
        })
        .collect();
    props.set_effects(ModelRc::new(VecModel::from(rows)));

    // パラメータ行はEFFECT_REGISTRYのParamSchema（label/kind/min/max）から生成する。
    // ハードコード撤廃: キー名の見た目・レンジは全てエフェクト定義側で決まる。
    let mut params = Vec::new();
    for (i, e) in instances.iter().enumerate() {
        let Some(meta) = find_effect(&e.effect_id) else {
            continue;
        };
        for schema in meta.params {
            let value = e
                .params
                .get(schema.key)
                .map(|p| match &p.static_value {
                    crate::ecs::types::Value::Number(n) => *n,
                    crate::ecs::types::Value::Bool(b) => {
                        if *b {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                })
                .unwrap_or(schema.default);
            params.push(ParamRow {
                effect_index: i as i32,
                key: schema.key.into(),
                label: schema.label.into(),
                value,
                kind: match schema.kind {
                    ParamKind::Float => 0,
                    ParamKind::Bool => 1,
                    ParamKind::Color => 2,
                },
                min: schema.min,
                max: schema.max,
            });
        }
    }
    props.set_params(ModelRc::new(VecModel::from(params)));
}
