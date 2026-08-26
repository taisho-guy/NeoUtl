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
    pub apply_mode: ApplyMode,
}

impl From<&Keyframe> for neoutl_schema::Keyframe {
    fn from(value: &Keyframe) -> Self {
        Self {
            frame: value.frame,
            value: value.value,
            engine_id: value.engine_id.clone(),
            engine_payload: value.engine_payload.clone(),
            edit_seq: value.edit_seq,
            apply_mode: match value.apply_mode {
                ApplyMode::Linear => neoutl_schema::ApplyMode::Linear as i32,
                ApplyMode::Interpolate => neoutl_schema::ApplyMode::Interpolate as i32,
            },
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
            apply_mode: match value.apply_mode() {
                neoutl_schema::ApplyMode::Linear => ApplyMode::Linear,
                neoutl_schema::ApplyMode::Interpolate => ApplyMode::Interpolate,
            },
        })
    }
}

impl Keyframe {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyMode {
    #[default]
    Linear,
    Interpolate,
}

impl ApplyMode {
    pub fn label(self) -> &'static str {
        match self {
            ApplyMode::Linear => "標準",
            ApplyMode::Interpolate => "補間",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            ApplyMode::Linear => ApplyMode::Interpolate,
            ApplyMode::Interpolate => ApplyMode::Linear,
        }
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
                let first_engine_id = &self.keyframes[0].engine_id;
                let engine = crate::easings::loader::by_id(first_engine_id);
                let raw_keyframes: Vec<(i32, f32, Vec<u8>)> = self
                    .keyframes
                    .iter()
                    .map(|k| (k.frame, k.value, k.engine_payload.clone()))
                    .collect();

                let val = if let Some(eng) = engine {
                    eng.evaluate(&raw_keyframes, frame, *base)
                } else {
                    *base
                };
                Value::Number(val)
            }
            other => other.clone(),
        }
    }

    pub fn set_static(&mut self, value: Value) {
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
                self.keyframes.push(Keyframe {
                    frame,
                    value,
                    engine_id,
                    engine_payload,
                    edit_seq,
                    apply_mode: ApplyMode::default(),
                });
                self.keyframes.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn shift_keyframes(&mut self, delta: i32) {
        for k in self.keyframes.iter_mut() {
            k.frame += delta;
        }
    }

    pub fn remove_keyframe(&mut self, frame: i32) {
        self.keyframes.retain(|k| k.frame != frame);
    }

    pub fn split_at(&mut self, split_frame: i32) -> EffectParam {
        let second_value = self.evaluate(split_frame);
        let second_keyframes: Vec<Keyframe> = self
            .keyframes
            .iter()
            .filter(|k| k.frame > split_frame)
            .cloned()
            .collect();
        self.keyframes.retain(|k| k.frame < split_frame);
        EffectParam {
            static_value: second_value,
            keyframes: second_keyframes,
        }
    }

    pub fn clamp_keyframes_to_range(
        &mut self,
        old_start: i32,
        old_end: i32,
        new_start: i32,
        new_end: i32,
    ) {
        if self.keyframes.is_empty() {
            return;
        }
        let base = match &self.static_value {
            Value::Number(b) => *b,
            _ => 0.0,
        };
        clamp_and_reseed_internal(
            &mut self.keyframes,
            old_start,
            old_end,
            new_start,
            new_end,
            base,
        );
    }
}

fn clamp_and_reseed_internal(
    keyframes: &mut Vec<Keyframe>,
    old_start: i32,
    old_end: i32,
    new_start: i32,
    new_end: i32,
    _base: f32,
) {
    if keyframes.is_empty() {
        return;
    }
    let old_len = (old_end - old_start).max(1) as f64;
    let new_len = (new_end - new_start).max(1) as f64;
    let scale = new_len / old_len;

    let start_engine_id = keyframes
        .first()
        .map(|k| k.engine_id.clone())
        .unwrap_or_default();
    let start_payload = keyframes
        .first()
        .map(|k| k.engine_payload.clone())
        .unwrap_or_default();
    let end_engine_id = keyframes
        .last()
        .map(|k| k.engine_id.clone())
        .unwrap_or_default();
    let end_payload = keyframes
        .last()
        .map(|k| k.engine_payload.clone())
        .unwrap_or_default();

    for k in keyframes.iter_mut() {
        let offset = (k.frame - old_start) as f64 * scale;
        k.frame = (new_start as f64 + offset).round() as i32;
        k.frame = k.frame.clamp(new_start, new_end);
    }
    keyframes.sort_by(|a, b| a.frame.cmp(&b.frame).then(a.edit_seq.cmp(&b.edit_seq)));

    let mut deduped: Vec<Keyframe> = Vec::with_capacity(keyframes.len());
    for k in keyframes.drain(..) {
        match deduped.last_mut() {
            Some(last) if last.frame == k.frame => {
                if k.edit_seq >= last.edit_seq {
                    *last = k;
                }
            }
            _ => deduped.push(k),
        }
    }

    deduped.retain(|k| k.frame != new_start && k.frame != new_end);
    deduped.insert(
        0,
        Keyframe {
            frame: new_start,
            value: 0.0,
            engine_id: start_engine_id,
            engine_payload: start_payload,
            edit_seq: next_edit_seq(),
            apply_mode: ApplyMode::default(),
        },
    );
    deduped.push(Keyframe {
        frame: new_end,
        value: 0.0,
        engine_id: end_engine_id,
        engine_payload: end_payload,
        edit_seq: next_edit_seq(),
        apply_mode: ApplyMode::default(),
    });

    *keyframes = deduped;
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
