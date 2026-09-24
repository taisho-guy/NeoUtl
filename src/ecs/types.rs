use crate::ecs::track::Track;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static EDIT_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn next_edit_seq() -> u64 {
    EDIT_SEQ.fetch_add(1, Ordering::Relaxed)
}

fn default_engine_id() -> String {
    "neoutl-easing-standard".to_string()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: i32,
    pub value: f32,
    #[serde(default = "default_engine_id")]
    pub engine_id: String,
    #[serde(default)]
    pub engine_payload: Vec<u8>,
    #[serde(default)]
    pub edit_seq: u64,
    #[serde(default)]
    pub control_frame: Option<i32>,
}

impl Keyframe {
    pub fn new(frame: i32, value: f32, engine_id: String, engine_payload: Vec<u8>) -> Self {
        Self {
            frame,
            value,
            engine_id,
            engine_payload,
            edit_seq: next_edit_seq(),
            control_frame: None,
        }
    }
}

impl From<&Keyframe> for neoutl_schema::Keyframe {
    fn from(value: &Keyframe) -> Self {
        Self {
            frame: value.frame,
            value: value.value,
            engine_id: value.engine_id.clone(),
            engine_payload: value.engine_payload.clone(),
            edit_seq: value.edit_seq,
            control_frame: value.control_frame,
        }
    }
}

impl TryFrom<&neoutl_schema::Keyframe> for Keyframe {
    type Error = String;

    fn try_from(value: &neoutl_schema::Keyframe) -> Result<Self, Self::Error> {
        Ok(Self {
            frame: value.frame,
            value: value.value,
            engine_id: value.engine_id.clone(),
            engine_payload: value.engine_payload.clone(),
            edit_seq: value.edit_seq,
            control_frame: value.control_frame,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Number(f32),
    Bool(bool),
    Text(String),
    FilePath(String),
    Enum(u32),
    TrackRef(i32),
}

impl From<&Value> for neoutl_schema::Value {
    fn from(value: &Value) -> Self {
        let kind = match value {
            Value::Number(v) => neoutl_schema::value::Kind::Number(*v),
            Value::Bool(v) => neoutl_schema::value::Kind::Boolean(*v),
            Value::Text(v) => neoutl_schema::value::Kind::Text(v.clone()),
            Value::FilePath(v) => neoutl_schema::value::Kind::FilePath(v.clone()),
            Value::Enum(v) => neoutl_schema::value::Kind::EnumValue(*v),
            Value::TrackRef(v) => neoutl_schema::value::Kind::TrackRef(*v),
        };
        Self { kind: Some(kind) }
    }
}

impl TryFrom<&neoutl_schema::Value> for Value {
    type Error = String;

    fn try_from(value: &neoutl_schema::Value) -> Result<Self, Self::Error> {
        match value.kind.as_ref() {
            Some(neoutl_schema::value::Kind::Number(v)) => Ok(Self::Number(*v)),
            Some(neoutl_schema::value::Kind::Boolean(v)) => Ok(Self::Bool(*v)),
            Some(neoutl_schema::value::Kind::Text(v)) => Ok(Self::Text(v.clone())),
            Some(neoutl_schema::value::Kind::FilePath(v)) => Ok(Self::FilePath(v.clone())),
            Some(neoutl_schema::value::Kind::EnumValue(v)) => Ok(Self::Enum(*v)),
            Some(neoutl_schema::value::Kind::TrackRef(v)) => Ok(Self::TrackRef(*v)),
            None => Err("missing Value kind".to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectParam {
    pub static_value: Value,
    pub keyframes: Vec<Keyframe>,
}

impl From<&EffectParam> for neoutl_schema::EffectParam {
    fn from(value: &EffectParam) -> Self {
        Self {
            static_value: Some(neoutl_schema::Value::from(&value.static_value)),
            keyframes: value
                .keyframes
                .iter()
                .map(neoutl_schema::Keyframe::from)
                .collect(),
        }
    }
}

impl TryFrom<&neoutl_schema::EffectParam> for EffectParam {
    type Error = String;

    fn try_from(value: &neoutl_schema::EffectParam) -> Result<Self, Self::Error> {
        Ok(Self {
            static_value: value
                .static_value
                .as_ref()
                .map(Value::try_from)
                .transpose()?
                .unwrap_or(Value::Number(0.0)),
            keyframes: value
                .keyframes
                .iter()
                .map(Keyframe::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl EffectParam {
    pub fn new(static_value: Value) -> Self {
        Self {
            static_value,
            keyframes: Vec::new(),
        }
    }

    pub fn evaluate(&self, frame: i32) -> Value {
        match &self.static_value {
            Value::Number(base) if !self.keyframes.is_empty() => {
                Value::Number(self.keyframes.evaluate(frame, *base))
            }
            other => other.clone(),
        }
    }

    pub fn set_static(&mut self, value: Value) {
        if let (Value::Number(v), Some(start)) = (&value, self.keyframes.first_mut()) {
            start.value = *v;
        }
        self.static_value = value;
    }

    pub fn set_keyframe(
        &mut self,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let edit_seq = next_edit_seq();
        match self.keyframes.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value;
                existing.engine_id = engine_id;
                existing.engine_payload = engine_payload;
                existing.edit_seq = edit_seq;
            }
            None => {
                self.keyframes
                    .push(Keyframe::new(frame, value, engine_id, engine_payload));
                self.keyframes.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn seed_start(&mut self, start: i32) {
        if let Value::Number(base) = self.static_value {
            self.keyframes.seed_start(start, base);
        }
    }

    pub fn shift_keyframes(&mut self, delta: i32) {
        self.keyframes.shift(delta);
    }

    pub fn remove_keyframe(&mut self, frame: i32) {
        if let Some(index) = self.keyframes.index_of(frame) {
            let _ = self.keyframes.remove_point(index);
        }
    }

    pub fn split_at(&mut self, split_frame: i32) -> EffectParam {
        let fallback = match self.static_value {
            Value::Number(base) => base,
            _ => 0.0,
        };
        let second_value = self.evaluate(split_frame);
        let keyframes = self.keyframes.split_at_frame(split_frame, fallback);
        EffectParam {
            static_value: second_value,
            keyframes,
        }
    }

    pub fn bind_range(&mut self, start: i32, end: i32) {
        self.keyframes.bind_range(start, end);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectInstance {
    pub effect_id: String,
    pub enabled: bool,
    pub params: HashMap<String, EffectParam>,
}

impl From<&EffectInstance> for neoutl_schema::EffectInstance {
    fn from(value: &EffectInstance) -> Self {
        Self {
            effect_id: value.effect_id.clone(),
            enabled: value.enabled,
            params: value
                .params
                .iter()
                .map(|(k, v)| (k.clone(), neoutl_schema::EffectParam::from(v)))
                .collect(),
        }
    }
}

impl TryFrom<&neoutl_schema::EffectInstance> for EffectInstance {
    type Error = String;

    fn try_from(value: &neoutl_schema::EffectInstance) -> Result<Self, Self::Error> {
        Ok(Self {
            effect_id: value.effect_id.clone(),
            enabled: value.enabled,
            params: value
                .params
                .iter()
                .map(|(k, v)| Ok((k.clone(), EffectParam::try_from(v)?)))
                .collect::<Result<HashMap<_, _>, String>>()?,
        })
    }
}

impl EffectInstance {
    pub fn new(effect_id: impl Into<String>) -> Self {
        Self {
            effect_id: effect_id.into(),
            enabled: true,
            params: HashMap::new(),
        }
    }
}
