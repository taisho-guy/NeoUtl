// src/ui/properties.rs
use crate::app_state::{self, SharedAppState};
use crate::ecs::{
    EcsWorld,
    components::ShapeParams,
    effects::EFFECT_REGISTRY,
    effects::EffectMetadata,
    effects::ParamKind,
    effects::find_effect,
    object_schema::{
        AUDIO_GROUP, AUDIO_SCHEMA, SHAPE_GROUP, SHAPE_SCHEMA, TEXT_GROUP, TEXT_SCHEMA,
        TRANSFORM_GROUP, TRANSFORM_SCHEMA, resolve_range,
    },
};
use crate::{CatalogRow, EffectRow, ParamRow, PropertiesWindow};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

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
        props.on_set_object_param(move |group, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param(&mut world, id as usize, group.as_str(), key.as_str(), value);
            drop(world);
            update_object_param_value(&p, group.as_str(), key.as_str(), value);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_object_param_bool(move |group, key, value| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param(
                &mut world,
                id as usize,
                group.as_str(),
                key.as_str(),
                if value { 1.0 } else { 0.0 },
            );
            refresh(&p, &world);
        });
    }

    {
        let state = state.clone();
        let pw = props.as_weak();
        props.on_set_text_content(move |text| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let cur = world.get_text(id as usize).unwrap_or_default();
            world.set_text(id as usize, text.to_string(), cur.x, cur.y, cur.font_size);
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
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_effect_param(id as usize, index as usize, key.as_str(), value);
            drop(world);
            update_effect_param_value(&p, index, key.as_str(), value);
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

/// object-params一行分の書き込みを、スキーマのgroup/keyから該当コンポーネントへ振り分ける。
/// UI生成はobject_schema.rsのテーブルのみに依存し、ここではキー名でのフィールド選択のみ行う。
fn apply_object_param(world: &mut EcsWorld, oid: usize, group: &str, key: &str, value: f32) {
    match group {
        TRANSFORM_GROUP => {
            let mut t = world.get_transform(oid).unwrap_or_default();
            match key {
                "x" => t.x = value,
                "y" => t.y = value,
                "z" => t.z = value,
                "scale_x" => t.scale_x = value,
                "scale_y" => t.scale_y = value,
                "rot_x" => t.rot_x = value,
                "rot_y" => t.rot_y = value,
                "rot_z" => t.rot_z = value,
                "opacity" => t.opacity = value,
                _ => return,
            }
            world.set_transform(oid, t);
        }
        TEXT_GROUP => {
            let cur = world.get_text(oid).unwrap_or_default();
            let (mut x, mut y, mut font_size) = (cur.x, cur.y, cur.font_size);
            match key {
                "text_x" => x = value,
                "text_y" => y = value,
                "font_size" => font_size = value,
                _ => return,
            }
            world.set_text(oid, cur.text, x, y, font_size);
        }
        SHAPE_GROUP => {
            let mut s: ShapeParams = world.get_shape(oid).unwrap_or_default();
            match key {
                "sides" => s.sides = value.max(3.0) as u32,
                "extrude_depth" => s.extrude_depth = value.max(0.0),
                "stroke_width" => s.stroke_width = value.max(0.0),
                "fill_r" => s.fill_color[0] = value,
                "fill_g" => s.fill_color[1] = value,
                "fill_b" => s.fill_color[2] = value,
                "fill_a" => s.fill_color[3] = value,
                _ => return,
            }
            world.set_shape(oid, s);
        }
        AUDIO_GROUP => {
            let cur = world.get_audio_params(oid);
            let (mut volume, mut pan, mut mute) = cur
                .map(|a| (a.volume, a.pan, a.mute))
                .unwrap_or((1.0, 0.0, false));
            match key {
                "volume" => volume = value,
                "pan" => pan = value,
                "mute" => mute = value > 0.5,
                _ => return,
            }
            world.set_audio_params(oid, volume, pan, mute);
        }
        // トランスフォーム/テキスト/図形/オーディオのいずれでもないgroupは、
        // プラグインObjectMeta.property_schema由来のキーとみなし汎用格納へ書き込む。
        // group名自体はUI表示にのみ使い、書き込み先の判定はkeyの存在有無に依存しない
        // （プラグインは任意のkey集合を持つため、host側で列挙できない）。
        _ => {
            world.set_plugin_param(oid, key, value);
        }
    }
}

/// スキーマ配列を現在値で解決し、ParamRow列へ展開する。
/// stage-relativeレンジ（X/Y/Z）はここでピクセル値へ確定する。
fn push_schema_rows(
    out: &mut Vec<ParamRow>,
    schema: &'static [crate::ecs::object_schema::ParamSchema],
    stage_w: f32,
    stage_h: f32,
    get: impl Fn(&str) -> f32,
) {
    for s in schema {
        let (min, max) = resolve_range(s.range, stage_w, stage_h);
        out.push(ParamRow {
            effect_index: -1,
            key: SharedString::from(s.key),
            label: SharedString::from(s.label),
            group: SharedString::from(s.group),
            value: get(s.key),
            kind: match s.kind {
                ParamKind::Float => 0,
                ParamKind::Bool => 1,
                ParamKind::Color => 2,
            },
            min,
            max,
        });
    }
}

/// プラグイン提供オブジェクトのObjectMeta.property_schemaをParamRow列へ展開する。
/// 現在値はPluginParams（未設定ならスキーマのdefault_float）から取得する。
/// neoutl_object_api::ParamKind::Enumはスライダー種別の専用UIを持たないため、
/// 暫定的にFloat（kind=0, step=1相当の整数入力）として表示する。
fn push_plugin_rows(out: &mut Vec<ParamRow>, world: &EcsWorld, oid: usize) {
    let Some(kind_id) = world.get_kind_id(oid) else {
        return;
    };
    let Some(plugin) = crate::objects::loader::by_kind_id(kind_id) else {
        return;
    };
    let meta = unsafe { &*((plugin.vtable.meta)()) };
    if meta.property_schema_ptr.is_null() || meta.property_schema_len == 0 {
        return;
    }
    let schema =
        unsafe { std::slice::from_raw_parts(meta.property_schema_ptr, meta.property_schema_len) };
    let current = world.get_plugin_params(oid).unwrap_or_default();
    let group = plugin.name.clone();

    for s in schema {
        let key = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(s.key.ptr, s.key.len))
        };
        let label = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(s.label.ptr, s.label.len))
        };
        let value = current.get(key).copied().unwrap_or(s.default_float);
        out.push(ParamRow {
            effect_index: -1,
            key: SharedString::from(key),
            label: SharedString::from(label),
            group: SharedString::from(group.as_str()),
            value,
            kind: match s.kind {
                neoutl_object_api::ParamKind::Float => 0,
                neoutl_object_api::ParamKind::Bool => 1,
                neoutl_object_api::ParamKind::Color => 2,
                neoutl_object_api::ParamKind::Enum => 0,
            },
            min: s.min,
            max: s.max,
        });
    }
}

/// object_paramsモデルの該当行(group/key一致)のみ値を書き換える。
/// ModelRcの同一性を保つため、Slint側のコンポーネント再構築(=ドラッグ状態/
/// テキスト選択状態の喪失)を発生させない。構造変化を伴わない値更新はこの経路を使う。
fn update_object_param_value(props: &PropertiesWindow, group: &str, key: &str, value: f32) {
    let model = props.get_object_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.group.as_str() == group && row.key.as_str() == key {
            row.value = value;
            model.set_row_data(i, row);
            return;
        }
    }
}

/// paramsモデル(エフェクトパラメータ)の該当行(effect_index/key一致)のみ値を書き換える。
/// update_object_param_valueと同一方針。
fn update_effect_param_value(props: &PropertiesWindow, effect_index: i32, key: &str, value: f32) {
    let model = props.get_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.effect_index == effect_index && row.key.as_str() == key {
            row.value = value;
            model.set_row_data(i, row);
            return;
        }
    }
}

fn refresh(props: &PropertiesWindow, world: &EcsWorld) {
    let id = props.get_object_id();
    if id < 0 {
        return;
    }
    let oid = id as usize;

    let project = world.get_project();
    let stage_w = project.width as f32;
    let stage_h = project.height as f32;
    props.set_stage_width(stage_w);
    props.set_stage_height(stage_h);

    let mut object_params: Vec<ParamRow> = Vec::new();

    if let Some(t) = world.get_transform(oid) {
        props.set_has_transform(true);
        push_schema_rows(
            &mut object_params,
            TRANSFORM_SCHEMA,
            stage_w,
            stage_h,
            |k| match k {
                "x" => t.x,
                "y" => t.y,
                "z" => t.z,
                "scale_x" => t.scale_x,
                "scale_y" => t.scale_y,
                "rot_x" => t.rot_x,
                "rot_y" => t.rot_y,
                "rot_z" => t.rot_z,
                "opacity" => t.opacity,
                _ => 0.0,
            },
        );
    } else {
        props.set_has_transform(false);
    }

    if let Some(text) = world.get_text(oid) {
        props.set_has_text(true);
        props.set_text_content(text.text.clone().into());
        push_schema_rows(
            &mut object_params,
            TEXT_SCHEMA,
            stage_w,
            stage_h,
            |k| match k {
                "text_x" => text.x,
                "text_y" => text.y,
                "font_size" => text.font_size,
                _ => 0.0,
            },
        );
    } else {
        props.set_has_text(false);
    }

    if let Some(shape) = world.get_shape(oid) {
        props.set_has_shape(true);
        push_schema_rows(
            &mut object_params,
            SHAPE_SCHEMA,
            stage_w,
            stage_h,
            |k| match k {
                "sides" => shape.sides as f32,
                "extrude_depth" => shape.extrude_depth,
                "stroke_width" => shape.stroke_width,
                "fill_r" => shape.fill_color[0],
                "fill_g" => shape.fill_color[1],
                "fill_b" => shape.fill_color[2],
                "fill_a" => shape.fill_color[3],
                _ => 0.0,
            },
        );
    } else {
        props.set_has_shape(false);
    }

    if let Some(audio) = world.get_audio_params(oid) {
        props.set_has_audio(true);
        push_schema_rows(
            &mut object_params,
            AUDIO_SCHEMA,
            stage_w,
            stage_h,
            |k| match k {
                "volume" => audio.volume,
                "pan" => audio.pan,
                "mute" => {
                    if audio.mute {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            },
        );
    } else {
        props.set_has_audio(false);
    }

    push_plugin_rows(&mut object_params, world, oid);

    props.set_object_params(ModelRc::new(VecModel::from(object_params)));

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
                group: meta.name.into(),
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
