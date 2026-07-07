// src/ecs/effects.rs
use shipyard::Component;

use crate::ecs::types::{EffectInstance, EffectParam, Value};

#[derive(Clone, Copy, Debug)]
pub struct EffectMetadata {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
}

pub const EFFECT_REGISTRY: &[EffectMetadata] = &[
    EffectMetadata {
        id: "border_blur",
        name: "BorderBlur",
        category: "Blur",
    },
    EffectMetadata {
        id: "chromatic_aberration",
        name: "ChromaticAberration",
        category: "Color",
    },
    EffectMetadata {
        id: "clipping",
        name: "Clipping",
        category: "Mask",
    },
    EffectMetadata {
        id: "color_correction",
        name: "ColorCorrection",
        category: "Color",
    },
    EffectMetadata {
        id: "diagonal_clipping",
        name: "DiagonalClipping",
        category: "Mask",
    },
    EffectMetadata {
        id: "diffuse_light",
        name: "DiffuseLight",
        category: "Light",
    },
    EffectMetadata {
        id: "directional_blur",
        name: "DirectionalBlur",
        category: "Blur",
    },
    EffectMetadata {
        id: "drop_shadow",
        name: "DropShadow",
        category: "Shadow",
    },
    EffectMetadata {
        id: "image_loop",
        name: "ImageLoop",
        category: "Utility",
    },
    EffectMetadata {
        id: "lens_blur",
        name: "LensBlur",
        category: "Blur",
    },
    EffectMetadata {
        id: "mosaic",
        name: "Mosaic",
        category: "Blur",
    },
    EffectMetadata {
        id: "motion_blur",
        name: "MotionBlur",
        category: "Blur",
    },
    EffectMetadata {
        id: "pixel_sorter",
        name: "PixelSorter",
        category: "Glitch",
    },
    EffectMetadata {
        id: "radial_blur",
        name: "RadialBlur",
        category: "Blur",
    },
    EffectMetadata {
        id: "transform",
        name: "Transform",
        category: "Geometry",
    },
    EffectMetadata {
        id: "vibration",
        name: "Vibration",
        category: "Distort",
    },
];

pub fn find_effect(id: &str) -> Option<&'static EffectMetadata> {
    EFFECT_REGISTRY.iter().find(|e| e.id == id)
}

/// Clipに付随するエフェクトの順序付きスタック。
/// AviQtl概念の「Effect[] (ordered list)」に相当。
#[derive(Clone, Debug, Default, Component)]
pub struct EffectStack(pub Vec<EffectInstance>);

impl EffectStack {
    pub fn push(&mut self, effect_id: impl Into<String>) {
        self.0.push(EffectInstance::new(effect_id));
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

    /// 旧UI互換: f32 を static に突っ込む（暫定）
    pub fn set_param_f32(&mut self, index: usize, key: &str, value: f32) {
        if let Some(e) = self.0.get_mut(index) {
            e.params.insert(
                key.to_owned(),
                EffectParam {
                    static_value: Value::Number(value),
                    keyframes: Vec::new(),
                },
            );
        }
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
