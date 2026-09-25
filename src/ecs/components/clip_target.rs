use super::param_access::ParamAccess;
use serde::{Deserialize, Serialize};
use shipyard::Component;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipMode {
    #[default]
    Alpha = 0,
    AlphaInvert = 1,
    Luminance = 2,
    LuminanceInvert = 3,
    Chroma = 4,
}

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct ClipTarget {
    pub enabled: bool,
    pub layer_count_down: u32,
    pub layer_count_up: u32,
    pub mode: ClipMode,
    pub chroma_hue: f32,
    pub chroma_tolerance: f32,
    pub blend_edge: bool,
    pub render_self: bool,
}

impl From<&ClipTarget> for neoutl_schema::ClipTarget {
    fn from(value: &ClipTarget) -> Self {
        Self {
            enabled: value.enabled,
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            mode: match value.mode {
                ClipMode::Alpha => neoutl_schema::ClipMode::Alpha as i32,
                ClipMode::AlphaInvert => neoutl_schema::ClipMode::AlphaInvert as i32,
                ClipMode::Luminance => neoutl_schema::ClipMode::Luminance as i32,
                ClipMode::LuminanceInvert => neoutl_schema::ClipMode::LuminanceInvert as i32,
                ClipMode::Chroma => neoutl_schema::ClipMode::Chroma as i32,
            },
            chroma_hue: value.chroma_hue,
            chroma_tolerance: value.chroma_tolerance,
            blend_edge: value.blend_edge,
            render_self: value.render_self,
        }
    }
}

impl TryFrom<&neoutl_schema::ClipTarget> for ClipTarget {
    type Error = String;

    fn try_from(value: &neoutl_schema::ClipTarget) -> Result<Self, Self::Error> {
        Ok(Self {
            enabled: value.enabled,
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            mode: match value.mode() {
                neoutl_schema::ClipMode::Alpha => ClipMode::Alpha,
                neoutl_schema::ClipMode::AlphaInvert => ClipMode::AlphaInvert,
                neoutl_schema::ClipMode::Luminance => ClipMode::Luminance,
                neoutl_schema::ClipMode::LuminanceInvert => ClipMode::LuminanceInvert,
                neoutl_schema::ClipMode::Chroma => ClipMode::Chroma,
            },
            chroma_hue: value.chroma_hue,
            chroma_tolerance: value.chroma_tolerance,
            blend_edge: value.blend_edge,
            render_self: value.render_self,
        })
    }
}

impl Default for ClipTarget {
    fn default() -> Self {
        Self {
            enabled: false,
            layer_count_down: 1,
            layer_count_up: 0,
            mode: ClipMode::Alpha,
            chroma_hue: 120.0,
            chroma_tolerance: 30.0,
            blend_edge: true,
            render_self: true,
        }
    }
}

impl ParamAccess for ClipTarget {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "enabled" => {
                if self.enabled {
                    1.0
                } else {
                    0.0
                }
            }
            "layer_count_down" => u32_param_to_f32(self.layer_count_down),
            "layer_count_up" => u32_param_to_f32(self.layer_count_up),
            "mode" => u8_param_to_f32(match self.mode {
                ClipMode::Alpha => 0,
                ClipMode::AlphaInvert => 1,
                ClipMode::Luminance => 2,
                ClipMode::LuminanceInvert => 3,
                ClipMode::Chroma => 4,
            }),
            "chroma_hue" => self.chroma_hue,
            "chroma_tolerance" => self.chroma_tolerance,
            "blend_edge" => {
                if self.blend_edge {
                    1.0
                } else {
                    0.0
                }
            }
            "render_self" => {
                if self.render_self {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "enabled" => self.enabled = value > 0.5,
            "layer_count_down" => {
                self.layer_count_down = f32_to_u32_param(value);
            }
            "layer_count_up" => {
                self.layer_count_up = f32_to_u32_param(value);
            }
            "mode" => {
                self.mode = match f32_to_u8_param(value) {
                    0 => ClipMode::Alpha,
                    1 => ClipMode::AlphaInvert,
                    2 => ClipMode::Luminance,
                    3 => ClipMode::LuminanceInvert,
                    _ => ClipMode::Chroma,
                }
            }
            "chroma_hue" => self.chroma_hue = value.rem_euclid(360.0),
            "chroma_tolerance" => self.chroma_tolerance = value.clamp(0.0, 180.0),
            "blend_edge" => self.blend_edge = value > 0.5,
            "render_self" => self.render_self = value > 0.5,
            _ => return false,
        }
        true
    }
}

fn u32_param_to_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn u8_param_to_f32(value: u8) -> f32 {
    f32::from(value)
}

fn f32_to_u32_param(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.round().to_string().parse().unwrap_or(u32::MAX)
}

fn f32_to_u8_param(value: f32) -> u8 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.round().to_string().parse().unwrap_or(u8::MAX)
}
