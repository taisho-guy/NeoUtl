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
}

impl Keyframe {
    pub fn new(frame: i32, value: f32, engine_id: String, engine_payload: Vec<u8>) -> Self {
        Self {
            frame,
            value,
            engine_id,
            engine_payload,
            edit_seq: next_edit_seq(),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectParam {
    pub static_value: Value,
    pub keyframes: Vec<Keyframe>,
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

    pub fn move_keyframe(&mut self, old_frame: i32, new_frame: i32) -> bool {
        if old_frame == new_frame {
            return true;
        }
        if self.keyframes.iter().any(|k| k.frame == new_frame) {
            return false;
        }
        let Some(k) = self.keyframes.iter_mut().find(|k| k.frame == old_frame) else {
            return false;
        };
        k.frame = new_frame;
        self.keyframes.sort_by_key(|k| k.frame);
        true
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
        },
    );
    deduped.push(Keyframe {
        frame: new_end,
        value: 0.0,
        engine_id: end_engine_id,
        engine_payload: end_payload,
        edit_seq: next_edit_seq(),
    });

    *keyframes = deduped;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectInstance {
    pub effect_id: String,
    pub enabled: bool,
    pub params: HashMap<String, EffectParam>,
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
