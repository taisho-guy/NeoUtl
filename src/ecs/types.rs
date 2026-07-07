use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Bool(bool),
    Text(String),
}

impl Value {
    pub fn as_number(&self) -> Option<f32> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Interpolation {
    Linear,
    Bezier {
        bzx1: f32,
        bzy1: f32,
        bzx2: f32,
        bzy2: f32,
    },
    Custom {
        expression: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    pub frame: i32,
    pub value: Value,
    pub interpolation: Interpolation,
}

pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn evaluate_linear_number(k0: &Keyframe, k1: &Keyframe, frame: i32) -> Option<Value> {
    let v0 = k0.value.as_number()?;
    let v1 = k1.value.as_number()?;
    let denom = (k1.frame - k0.frame).max(1) as f32;
    let t = (frame - k0.frame) as f32 / denom;
    Some(Value::Number(lerp_f32(v0, v1, t.clamp(0.0, 1.0))))
}

pub fn evaluate_keyframes_number(keyframes: &[Keyframe], frame: i32) -> Option<Value> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 {
        return Some(keyframes[0].value.clone());
    }

    // keyframes are expected sorted by frame ascending
    let mut prev = &keyframes[0];
    for k in &keyframes[1..] {
        if frame < k.frame {
            let k0 = prev;
            let k1 = k;
            return match &k1.interpolation {
                Interpolation::Linear => evaluate_linear_number(k0, k1, frame),
                Interpolation::Bezier { .. } => evaluate_linear_number(k0, k1, frame),
                Interpolation::Custom { .. } => evaluate_linear_number(k0, k1, frame),
            };
        }
        prev = k;
    }

    Some(keyframes.last()?.value.clone())
}

#[derive(Clone, Debug)]
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
        if self.keyframes.is_empty() {
            return self.static_value.clone();
        }
        evaluate_keyframes_number(&self.keyframes, frame)
            .unwrap_or_else(|| self.static_value.clone())
    }
}

#[derive(Clone, Debug)]
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
