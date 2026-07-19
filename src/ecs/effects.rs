use shipyard::Component;

use crate::ecs::types::{EffectInstance, EffectParam, Value};

/// エフェクトメタデータ・パラメータスキーマはneoutl-effect-api経由のcdylibプラグインが
/// 保持する（EFFECT_REGISTRY静的配列は廃止）。ホストはcrate::effects::loaderへ委譲する。
pub use neoutl_effect_api::{EffectMeta, ParamKind};
pub type ParamSchema = neoutl_effect_api::EffectParamSchema;

pub fn find_effect(id: &str) -> Option<&'static EffectMeta> {
    crate::effects::loader::by_id(id).map(|p| unsafe { &*((p.vtable.meta)()) })
}

/// EffectMeta.param_schema_ptr/lenから'staticなParamSchemaスライスを得る。
/// # Safety
/// meta.param_schema_ptrはparam_schema_len個の有効なParamSchemaを指し続けること
/// （cdylibプラグインのstatic配列を指すため、プロセス生存中は常に有効）。
pub fn param_schema(meta: &EffectMeta) -> &'static [ParamSchema] {
    if meta.param_schema_ptr.is_null() || meta.param_schema_len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(meta.param_schema_ptr, meta.param_schema_len) }
}

/// Clipに付随するエフェクトの順序付きスタック。
/// AviQtl概念の「Effect[] (ordered list)」に相当。
#[derive(Clone, Debug, Default, Component)]
pub struct EffectStack(pub Vec<EffectInstance>);

impl EffectStack {
    /// 追加時にスキーマのdefault値をパラメータ初期値として展開する。
    pub fn push(&mut self, effect_id: impl Into<String>) {
        let effect_id = effect_id.into();
        let mut instance = EffectInstance::new(effect_id.clone());
        if let Some(meta) = find_effect(&effect_id) {
            for p in param_schema(meta) {
                let key = unsafe { p.key.as_str() }.to_owned();
                let value = match p.kind {
                    ParamKind::Bool => Value::Bool(p.default_float != 0.0),
                    _ => Value::Number(p.default_float),
                };
                instance.params.insert(key, EffectParam::new(value));
            }
        }
        self.0.push(instance);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    pub fn set_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(e) = self.0.get_mut(index) {
            e.enabled = enabled;
        }
    }

    pub fn set_param_f32(&mut self, index: usize, key: &str, value: f32) {
        self.set_param_value(index, key, Value::Number(value));
    }

    pub fn set_param_bool(&mut self, index: usize, key: &str, value: bool) {
        self.set_param_value(index, key, Value::Bool(value));
    }

    pub fn set_param_value(&mut self, index: usize, key: &str, value: Value) {
        if let Some(e) = self.0.get_mut(index) {
            e.params.insert(
                key.to_owned(),
                EffectParam {
                    static_value: value,
                    keyframes: Vec::new(),
                },
            );
        }
    }
}

/// 有効エフェクトのパラメータを「指定フレームで評価した値」で列挙。
/// GPU実行は renderer 側の責務。
pub fn compute_effect_params_at(
    stack: &EffectStack,
    frame: i32,
) -> Vec<(String, std::collections::HashMap<String, Value>)> {
    stack
        .0
        .iter()
        .filter(|e| e.enabled)
        .map(|e| {
            let mut evaluated = std::collections::HashMap::new();
            for (k, p) in &e.params {
                evaluated.insert(k.clone(), p.evaluate(frame));
            }
            (e.effect_id.clone(), evaluated)
        })
        .collect()
}
