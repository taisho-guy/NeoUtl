// src/ecs/effects.rs
use shipyard::Component;

use crate::ecs::types::{EffectInstance, EffectParam, Value};

/// 設定ダイアログUI生成用のパラメータ種別（ホスト内蔵エフェクト用。cdylib跨ぎはしない）。
/// Textはproperties.rs側でParamRow.textを介した文字列専用経路として扱う
/// （数値min/max/stepは不使用）。エフェクトスタック(EFFECT_REGISTRY)は現状Text未使用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Float,
    Bool,
    Color,
    Text,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
}

const fn float_param(
    key: &'static str,
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Float,
        min,
        max,
        step: (max - min) / 100.0,
        default,
    }
}
const fn bool_param(key: &'static str, label: &'static str, default: bool) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default: if default { 1.0 } else { 0.0 },
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EffectMetadata {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub params: &'static [ParamSchema],
}

pub const EFFECT_REGISTRY: &[EffectMetadata] = &[
    EffectMetadata {
        id: "border_blur",
        name: "BorderBlur",
        category: "Blur",
        params: &[
            float_param("radius", "半径", 0.0, 100.0, 8.0),
            float_param("border_width", "境界幅", 0.0, 200.0, 20.0),
        ],
    },
    EffectMetadata {
        id: "chromatic_aberration",
        name: "ChromaticAberration",
        category: "Color",
        params: &[float_param("shift", "ずれ量", 0.0, 50.0, 4.0)],
    },
    EffectMetadata {
        id: "clipping",
        name: "Clipping",
        category: "Mask",
        params: &[
            float_param("left", "左", 0.0, 1.0, 0.0),
            float_param("top", "上", 0.0, 1.0, 0.0),
            float_param("right", "右", 0.0, 1.0, 1.0),
            float_param("bottom", "下", 0.0, 1.0, 1.0),
            bool_param("invert", "反転", false),
        ],
    },
    EffectMetadata {
        id: "color_correction",
        name: "ColorCorrection",
        category: "Color",
        params: &[
            float_param("brightness", "明度", -1.0, 1.0, 0.0),
            float_param("contrast", "コントラスト", -1.0, 1.0, 0.0),
            float_param("saturation", "彩度", -1.0, 1.0, 0.0),
            float_param("hue", "色相", -180.0, 180.0, 0.0),
        ],
    },
    EffectMetadata {
        id: "diagonal_clipping",
        name: "DiagonalClipping",
        category: "Mask",
        params: &[
            float_param("angle", "角度", -180.0, 180.0, 45.0),
            float_param("offset", "オフセット", -1.0, 1.0, 0.0),
        ],
    },
    EffectMetadata {
        id: "diffuse_light",
        name: "DiffuseLight",
        category: "Light",
        params: &[
            float_param("intensity", "強度", 0.0, 5.0, 1.0),
            float_param("angle", "角度", -180.0, 180.0, 45.0),
        ],
    },
    EffectMetadata {
        id: "directional_blur",
        name: "DirectionalBlur",
        category: "Blur",
        params: &[
            float_param("angle", "角度", -180.0, 180.0, 0.0),
            float_param("distance", "距離", 0.0, 200.0, 20.0),
        ],
    },
    EffectMetadata {
        id: "drop_shadow",
        name: "DropShadow",
        category: "Shadow",
        params: &[
            float_param("offset_x", "X方向", -100.0, 100.0, 8.0),
            float_param("offset_y", "Y方向", -100.0, 100.0, 8.0),
            float_param("blur", "ぼかし", 0.0, 100.0, 6.0),
            float_param("opacity", "不透明度", 0.0, 1.0, 0.6),
        ],
    },
    EffectMetadata {
        id: "image_loop",
        name: "ImageLoop",
        category: "Utility",
        params: &[bool_param("enabled", "有効", true)],
    },
    EffectMetadata {
        id: "lens_blur",
        name: "LensBlur",
        category: "Blur",
        params: &[float_param("radius", "半径", 0.0, 100.0, 10.0)],
    },
    EffectMetadata {
        id: "mosaic",
        name: "Mosaic",
        category: "Blur",
        params: &[float_param("cell_size", "セルサイズ", 1.0, 200.0, 16.0)],
    },
    EffectMetadata {
        id: "motion_blur",
        name: "MotionBlur",
        category: "Blur",
        params: &[float_param(
            "shutter_angle",
            "シャッター角",
            0.0,
            360.0,
            180.0,
        )],
    },
    EffectMetadata {
        id: "pixel_sorter",
        name: "PixelSorter",
        category: "Glitch",
        params: &[
            float_param("threshold", "閾値", 0.0, 1.0, 0.5),
            float_param("angle", "角度", -180.0, 180.0, 0.0),
        ],
    },
    EffectMetadata {
        id: "radial_blur",
        name: "RadialBlur",
        category: "Blur",
        params: &[
            float_param("center_x", "中心X", 0.0, 1.0, 0.5),
            float_param("center_y", "中心Y", 0.0, 1.0, 0.5),
            float_param("strength", "強さ", 0.0, 100.0, 10.0),
        ],
    },
    EffectMetadata {
        id: "transform",
        name: "Transform",
        category: "Geometry",
        params: &[
            float_param("scale", "拡大率", 0.0, 10.0, 1.0),
            float_param("rotation", "回転", -360.0, 360.0, 0.0),
        ],
    },
    EffectMetadata {
        id: "vibration",
        name: "Vibration",
        category: "Distort",
        params: &[
            float_param("amplitude", "振幅", 0.0, 100.0, 4.0),
            float_param("frequency", "周波数", 0.0, 60.0, 10.0),
        ],
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
    /// 追加時にスキーマのdefault値をパラメータ初期値として展開する。
    pub fn push(&mut self, effect_id: impl Into<String>) {
        let effect_id = effect_id.into();
        let mut instance = EffectInstance::new(effect_id.clone());
        if let Some(meta) = find_effect(&effect_id) {
            for p in meta.params {
                instance
                    .params
                    .insert(p.key.to_owned(), EffectParam::new(Value::Number(p.default)));
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
