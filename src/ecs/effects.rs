// src/ecs/effects.rs
use shipyard::Component;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct EffectInstance {
    pub effect_id: String,
    pub enabled: bool,
    pub params: HashMap<String, f32>,
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

    pub fn set_param(&mut self, index: usize, key: &str, value: f32) {
        if let Some(e) = self.0.get_mut(index) {
            e.params.insert(key.to_owned(), value);
        }
    }
}

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

/// 有効エフェクトのパラメータを列挙順に合成する。GPU実行はrenderer側の責務とする。
pub fn compute_effect_params(stack: &EffectStack) -> Vec<(String, HashMap<String, f32>)> {
    stack
        .0
        .iter()
        .filter(|e| e.enabled)
        .map(|e| (e.effect_id.clone(), e.params.clone()))
        .collect()
}
