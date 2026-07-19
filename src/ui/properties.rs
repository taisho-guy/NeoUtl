use crate::app_state::{self, SharedAppState};
use crate::ecs::{
    EcsWorld,
    components::{ParamAccess, ShapeParams},
    effects::{find_effect, param_schema},
    object_schema::{
        AUDIO_GROUP, AUDIO_SCHEMA, SHAPE_GROUP, SHAPE_SCHEMA, TEXT_GROUP, TEXT_SCHEMA,
        TRANSFORM_GROUP, TRANSFORM_SCHEMA, resolve_range,
    },
};
use crate::{CatalogRow, EffectRow, ParamRow, PropertiesWindow};
use neoutl_shared_abi::ParamKind;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

pub fn setup(props: &PropertiesWindow, state: SharedAppState) {
    let catalog: Vec<CatalogRow> = crate::effects::loader::registry()
        .iter()
        .map(|p| CatalogRow {
            id: p.id.clone().into(),
            name: p.name.clone().into(),
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
        props.on_set_object_param_text(move |group, key, text| {
            let Some(p) = pw.upgrade() else { return };
            let id = p.get_object_id();
            if id < 0 {
                return;
            }
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            apply_object_param_text(
                &mut world,
                id as usize,
                group.as_str(),
                key.as_str(),
                text.as_str(),
            );
            drop(world);
            update_object_param_text(&p, group.as_str(), key.as_str(), text.as_str());
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
            app_state::snapshot_before_edit(&state);
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
            app_state::snapshot_before_edit(&state);
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
            app_state::snapshot_before_edit(&state);
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
            app_state::snapshot_before_edit(&state);
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
/// key単位のフィールド選択はParamAccess::set_param（各コンポーネント定義側）に委譲する。
/// ここではgroup名から対象コンポーネントを選び、読み出し→trait経由の書き込み→保存のみを行う。
fn apply_object_param(world: &mut EcsWorld, oid: usize, group: &str, key: &str, value: f32) {
    match group {
        TRANSFORM_GROUP => {
            let mut t = world.get_transform(oid).unwrap_or_default();
            if t.set_param(key, value) {
                world.set_transform(oid, t);
            }
        }
        TEXT_GROUP => {
            let mut t = world.get_text(oid).unwrap_or_default();
            if t.set_param(key, value) {
                world.set_text(oid, t.text, t.x, t.y, t.font_size);
            }
        }
        SHAPE_GROUP => {
            let mut s: ShapeParams = world.get_shape(oid).unwrap_or_default();
            if s.set_param(key, value) {
                world.set_shape(oid, s);
            }
        }
        AUDIO_GROUP => {
            let mut a = world.get_audio_params(oid).unwrap_or_default();
            if a.set_param(key, value) {
                world.set_audio_params(oid, a.volume, a.pan, a.mute);
            }
        }
        _ => {
            world.set_plugin_param(oid, key, value);
        }
    }
}

/// ParamKind::Text専用の書き込み経路。現状ホスト内蔵ではTEXT_GROUPの"text"キーのみが対象。
fn apply_object_param_text(world: &mut EcsWorld, oid: usize, group: &str, key: &str, text: &str) {
    if group == TEXT_GROUP && key == "text" {
        let cur = world.get_text(oid).unwrap_or_default();
        world.set_text(oid, text.to_owned(), cur.x, cur.y, cur.font_size);
    }
}

/// スキーマ配列を現在値で解決し、ParamRow列へ展開する。
/// stage-relativeレンジ（X/Y/Z）はここでピクセル値へ確定する。
/// get_text: kind==Textの行にのみ使用。対象外keyにはNoneを返せばよい。
fn push_schema_rows(
    out: &mut Vec<ParamRow>,
    schema: &'static [crate::ecs::object_schema::ParamSchema],
    stage_w: f32,
    stage_h: f32,
    get: impl Fn(&str) -> f32,
    get_text: impl Fn(&str) -> Option<String>,
) {
    for s in schema {
        let (min, max) = resolve_range(s.range, stage_w, stage_h);
        out.push(ParamRow {
            effect_index: -1,
            key: SharedString::from(s.key),
            label: SharedString::from(s.label),
            group: SharedString::from(s.group),
            value: if s.kind == ParamKind::Text {
                0.0
            } else {
                get(s.key)
            },
            kind: match s.kind {
                ParamKind::Float => 0,
                ParamKind::Bool => 1,
                ParamKind::Color => 2,
                ParamKind::Text => 3,
                ParamKind::Enum => 0,
            },
            min,
            max,
            text: SharedString::from(get_text(s.key).unwrap_or_default()),
        });
    }
}

/// C ABI越しのParamSchema配列（オブジェクトプラグイン・エフェクトプラグイン共通形式）を
/// 現在値で解決しParamRow列へ展開する。両プラグイン種別はneoutl-shared-abi::ParamSchemaを
/// 共有するため、この一関数で処理できる（Phase6: push_plugin_rowsとエフェクトパラメータ
/// 生成ループの重複を解消）。
fn push_c_abi_param_rows(
    out: &mut Vec<ParamRow>,
    schema: &[neoutl_shared_abi::ParamSchema],
    group: &str,
    effect_index: i32,
    current: impl Fn(&str) -> f32,
) {
    for s in schema {
        let key = unsafe { s.key.as_str() };
        let label = unsafe { s.label.as_str() };
        let value = current(key);
        out.push(ParamRow {
            effect_index,
            key: SharedString::from(key),
            label: SharedString::from(label),
            group: SharedString::from(group),
            value,
            kind: match s.kind {
                ParamKind::Float => 0,
                ParamKind::Bool => 1,
                ParamKind::Color => 2,
                ParamKind::Enum => 0,
                ParamKind::Text => 3,
            },
            min: s.min,
            max: s.max,
            text: SharedString::default(),
        });
    }
}

/// プラグイン提供オブジェクトのObjectMeta.property_schemaをParamRow列へ展開する。
/// 現在値はPluginParams（未設定ならスキーマのdefault_float）から取得する。
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
    push_c_abi_param_rows(out, schema, &plugin.name, -1, |key| {
        current.get(key).copied().unwrap_or_else(|| {
            schema
                .iter()
                .find(|s| unsafe { s.key.as_str() } == key)
                .map(|s| s.default_float)
                .unwrap_or(0.0)
        })
    });
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

/// object_paramsモデルの該当行(group/key一致)のtextフィールドのみ書き換える。
/// kind==3(Text)行専用。update_object_param_valueと同一方針。
fn update_object_param_text(props: &PropertiesWindow, group: &str, key: &str, text: &str) {
    let model = props.get_object_params();
    for i in 0..model.row_count() {
        let Some(mut row) = model.row_data(i) else {
            continue;
        };
        if row.group.as_str() == group && row.key.as_str() == key {
            row.text = SharedString::from(text);
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
            |k| t.get_param(k).unwrap_or(0.0),
            |_| None,
        );
    } else {
        props.set_has_transform(false);
    }

    if let Some(text) = world.get_text(oid) {
        props.set_has_text(true);
        let body = text.text.clone();
        push_schema_rows(
            &mut object_params,
            TEXT_SCHEMA,
            stage_w,
            stage_h,
            |k| text.get_param(k).unwrap_or(0.0),
            |k| (k == "text").then(|| body.clone()),
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
            |k| shape.get_param(k).unwrap_or(0.0),
            |_| None,
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
            |k| audio.get_param(k).unwrap_or(0.0),
            |_| None,
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
                .map(|m| m.name)
                .unwrap_or(e.effect_id.as_str())
                .into(),
            enabled: e.enabled,
        })
        .collect();
    props.set_effects(ModelRc::new(VecModel::from(rows)));

    let mut params = Vec::new();
    for (i, e) in instances.iter().enumerate() {
        let Some(meta) = find_effect(&e.effect_id) else {
            continue;
        };
        let schema = param_schema(meta);
        push_c_abi_param_rows(&mut params, schema, meta.name, i as i32, |key| {
            e.params
                .get(key)
                .map(|p| match &p.static_value {
                    crate::ecs::types::Value::Number(n) => *n,
                    crate::ecs::types::Value::Bool(b) if *b => 1.0,
                    crate::ecs::types::Value::Bool(_) => 0.0,
                    _ => 0.0,
                })
                .unwrap_or_else(|| {
                    schema
                        .iter()
                        .find(|s| unsafe { s.key.as_str() } == key)
                        .map(|s| s.default_float)
                        .unwrap_or(0.0)
                })
        });
    }
    props.set_params(ModelRc::new(VecModel::from(params)));
}
