use shipyard::Component;

use crate::ecs::types::{EffectInstance, EffectParam, Value};

pub use neoutl_shared_abi::{ParamKind, ParamRowOwned};

pub fn find_effect(id: &str) -> Option<std::sync::Arc<crate::effects::EffectSource>> {
    crate::effects::loader::by_id(id)
}

pub fn param_schema(source: &crate::effects::EffectSource) -> Vec<ParamRowOwned> {
    source.param_schema()
}

#[derive(Clone, Debug, Default, Component)]
pub struct EffectStack(pub Vec<EffectInstance>);

impl EffectStack {
    pub fn push(&mut self, effect_id: impl Into<String>) {
        let effect_id = effect_id.into();
        let mut instance = EffectInstance::new(effect_id.clone());
        if let Some(source) = find_effect(&effect_id) {
            for p in param_schema(&source) {
                let value = match p.kind {
                    ParamKind::Bool => Value::Bool(p.default_float != 0.0),
                    ParamKind::Enum => Value::Enum(p.default_float as u32),
                    ParamKind::Text => Value::Text(String::new()),
                    ParamKind::FilePath => Value::FilePath(String::new()),
                    ParamKind::Track => Value::TrackRef(-1),
                    ParamKind::Float | ParamKind::Color => Value::Number(p.default_float),
                    ParamKind::Separator | ParamKind::Group => continue,
                    ParamKind::Folder => Value::FilePath(String::new()),
                };
                instance.params.insert(p.key, EffectParam::new(value));
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

    pub fn set_param_text(&mut self, index: usize, key: &str, value: String) {
        self.set_param_value(index, key, Value::Text(value));
    }

    pub fn set_param_path(&mut self, index: usize, key: &str, value: String) {
        self.set_param_value(index, key, Value::FilePath(value));
    }

    pub fn set_param_enum(&mut self, index: usize, key: &str, value: u32) {
        self.set_param_value(index, key, Value::Enum(value));
    }

    pub fn set_param_track_ref(&mut self, index: usize, key: &str, value: i32) {
        self.set_param_value(index, key, Value::TrackRef(value));
    }

    pub fn set_param_value(&mut self, index: usize, key: &str, value: Value) {
        if let Some(e) = self.0.get_mut(index) {
            match e.params.get_mut(key) {
                Some(p) => p.set_static(value),
                None => {
                    e.params.insert(key.to_owned(), EffectParam::new(value));
                }
            }
        }
    }

    pub fn set_keyframe(
        &mut self,
        index: usize,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        if let Some(e) = self.0.get_mut(index) {
            e.params
                .entry(key.to_owned())
                .or_insert_with(|| EffectParam::new(Value::Number(value)))
                .set_keyframe(frame, value, engine_id, engine_payload);
        }
    }

    pub fn set_apply_mode(
        &mut self,
        index: usize,
        key: &str,
        frame: i32,
        mode: crate::ecs::types::ApplyMode,
    ) {
        if let Some(e) = self.0.get_mut(index)
            && let Some(p) = e.params.get_mut(key)
        {
            p.set_apply_mode(frame, mode);
        }
    }

    pub fn remove_keyframe(&mut self, index: usize, key: &str, frame: i32) {
        if let Some(e) = self.0.get_mut(index)
            && let Some(p) = e.params.get_mut(key)
        {
            p.remove_keyframe(frame);
        }
    }

    pub fn split_at(&mut self, split_frame: i32) -> EffectStack {
        let second: Vec<EffectInstance> = self
            .0
            .iter_mut()
            .map(|e| {
                let mut second_params = std::collections::HashMap::new();
                for (key, param) in e.params.iter_mut() {
                    second_params.insert(key.clone(), param.split_at(split_frame));
                }
                EffectInstance {
                    effect_id: e.effect_id.clone(),
                    enabled: e.enabled,
                    params: second_params,
                }
            })
            .collect();
        EffectStack(second)
    }
}

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
