use super::param_access::ParamAccess;
use serde::{Deserialize, Serialize};
use shipyard::Component;

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct AudioParams {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
}

impl From<&AudioParams> for neoutl_schema::AudioParams {
    fn from(value: &AudioParams) -> Self {
        Self {
            volume: value.volume,
            pan: value.pan,
            mute: value.mute,
        }
    }
}

impl TryFrom<&neoutl_schema::AudioParams> for AudioParams {
    type Error = String;

    fn try_from(value: &neoutl_schema::AudioParams) -> Result<Self, Self::Error> {
        Ok(Self {
            volume: value.volume,
            pan: value.pan,
            mute: value.mute,
        })
    }
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            mute: false,
        }
    }
}

impl ParamAccess for AudioParams {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "volume" => self.volume,
            "pan" => self.pan,
            "mute" => {
                if self.mute {
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
            "volume" => self.volume = value,
            "pan" => self.pan = value,
            "mute" => self.mute = value > 0.5,
            _ => return false,
        }
        true
    }
}
