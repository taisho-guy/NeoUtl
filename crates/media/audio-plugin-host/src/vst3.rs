use crate::{NeoPlugin, error::PluginError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vst3_host::{audio::AudioBuffers, plugin::Plugin, simple};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub is_bypass: bool,
}

pub struct Vst3Wrapper(Plugin);

impl Vst3Wrapper {
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        Ok(Self(simple::load_plugin_isolated(path)?))
    }
}

impl NeoPlugin for Vst3Wrapper {
    fn start(&mut self) -> Result<(), PluginError> {
        self.0.start_processing().map_err(Into::into)
    }

    fn stop(&mut self) -> Result<(), PluginError> {
        self.0.stop_processing().map_err(Into::into)
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], frames: usize) {
        let sample_rate = self.0.sample_rate();
        let mut buffers = AudioBuffers::new(inputs.len(), outputs.len(), frames, sample_rate);
        for (dst, src) in buffers.inputs.iter_mut().zip(inputs.iter()) {
            dst[..frames].copy_from_slice(&src[..frames]);
        }
        let _ = self.0.process_audio(&mut buffers);
        for (dst, src) in outputs.iter_mut().zip(buffers.outputs.iter()) {
            dst[..frames].copy_from_slice(&src[..frames]);
        }
    }

    fn set_parameter(&mut self, id: u32, value: f64) -> Result<(), PluginError> {
        self.0.set_parameter(id, value).map_err(Into::into)
    }

    fn param_info(&self) -> Vec<PluginParamInfo> {
        self.0
            .get_parameters()
            .map(|params| {
                params
                    .into_iter()
                    .map(|p| PluginParamInfo {
                        id: p.id,
                        name: p.name,
                        min: p.min,
                        max: p.max,
                        default: p.default,
                        is_bypass: p.is_bypass,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
