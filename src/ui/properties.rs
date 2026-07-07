// src/ui/properties.rs
use crate::ecs::{
    EcsWorld, effects::EFFECT_REGISTRY, effects::EffectMetadata, effects::find_effect,
};
use crate::{CatalogRow, EffectRow, ParamRow, PropertiesWindow};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::sync::{Arc, Mutex};

pub fn setup(props: &PropertiesWindow, world_holder: Arc<Mutex<EcsWorld>>) {
    let catalog: Vec<CatalogRow> = EFFECT_REGISTRY
        .iter()
        .map(|m| CatalogRow {
            id: m.id.into(),
            name: m.name.into(),
        })
        .collect();
    props.set_effect_catalog(ModelRc::new(VecModel::from(catalog)));

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_set_transform(move |x, y, z, sx, sy, rx, ry, rz, op| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let mut world = wc.lock().unwrap();
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
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_set_text(move |text, x, y, font_size| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .set_text(id as usize, text.to_string(), x, y, font_size);
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_set_audio(move |volume, pan, mute| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .set_audio_params(id as usize, volume, pan, mute);
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_set_effect_enabled(move |index, enabled| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .set_effect_enabled(id as usize, index as usize, enabled);
            refresh(&p, &wc.lock().unwrap());
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_remove_effect(move |index| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .remove_effect(id as usize, index as usize);
            refresh(&p, &wc.lock().unwrap());
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_set_param(move |index, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .set_effect_param(id as usize, index as usize, key.as_str(), value);
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_add_effect(move |effect_id| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .add_effect(id as usize, effect_id.as_str());
            refresh(&p, &wc.lock().unwrap());
        });
    }

    {
        let wc = world_holder.clone();
        let pw = props.as_weak();
        props.on_move_effect(move |from, to| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 || from < 0 || to < 0 {
                return;
            }
            wc.lock()
                .unwrap()
                .reorder_effect(id as usize, from as usize, to as usize);
            refresh(&p, &wc.lock().unwrap());
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

    let mut params = Vec::new();
    for (i, e) in instances.iter().enumerate() {
        let mut keys: Vec<&String> = e.params.keys().collect();
        keys.sort();
        for k in keys {
            let value = match e.params[k].static_value {
                crate::ecs::types::Value::Number(n) => n,
                _ => 0.0,
            };
            params.push(ParamRow {
                effect_index: i as i32,
                key: k.clone().into(),
                value,
            });
        }
    }
    props.set_params(ModelRc::new(VecModel::from(params)));
}
