mod crash;
mod process;
mod registry;
mod types;

pub use crash::{CRASH_THRESHOLD, block_plugin, is_blocked, record_crash};
pub use registry::{catalog, invalidate};
pub use types::{HostError, PluginCatalogEntry, PluginFormat, PluginParamInfo};

use process::PluginProcess;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct PluginHost {
    binary_path: PathBuf,
    sample_rate: f64,
    buffer_size: usize,
    instances: HashMap<u32, PluginProcess>,
    next_id: u32,
}

impl PluginHost {
    pub fn new(binary_path: PathBuf, sample_rate: f64, buffer_size: usize) -> Self {
        Self {
            binary_path,
            sample_rate,
            buffer_size,
            instances: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    pub fn set_buffer_size(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
    }

    pub fn add_plugin(
        &mut self,
        format: PluginFormat,
        plugin_spec: &str,
    ) -> Result<u32, HostError> {
        if crash::is_blocked(plugin_spec) {
            return Err(HostError::Blocked(plugin_spec.to_string()));
        }
        let id = self.next_id;
        let instance_id = id.to_string();
        let process = PluginProcess::spawn(
            &self.binary_path,
            format,
            plugin_spec,
            &instance_id,
            self.sample_rate,
            self.buffer_size,
            2,
            2,
        )?;
        self.instances.insert(id, process);
        self.next_id += 1;
        Ok(id)
    }

    pub fn remove_plugin(&mut self, id: u32) -> Result<(), HostError> {
        self.instances
            .remove(&id)
            .map(|_| ())
            .ok_or(HostError::UnknownInstance(id))
    }

    pub fn set_parameter_value(&mut self, id: u32, param_id: u32, value: f32) {
        if let Some(p) = self.instances.get(&id) {
            p.set_parameter_value(param_id, value);
        }
    }

    pub fn full_param_info_list(&mut self, id: u32) -> Vec<PluginParamInfo> {
        match self.instances.get(&id) {
            Some(p) => p.full_param_info_list().unwrap_or_default(),
            None => Vec::new(),
        }
    }

    pub fn process_stereo(
        &mut self,
        id: u32,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        frames: usize,
    ) -> Result<(), HostError> {
        if matches!(self.instances.get(&id), Some(p) if !p.is_alive()) {
            if let Some(dead) = self.instances.remove(&id) {
                eprintln!(
                    "[maolan-host-adapter] 停止済プラグインプロセス除去: id={} shm={}",
                    id,
                    dead.shm_name()
                );
            }
            return Err(HostError::ProcessDead);
        }
        let process = self
            .instances
            .get_mut(&id)
            .ok_or(HostError::UnknownInstance(id))?;
        let result = process.process_stereo(in_l, in_r, out_l, out_r, frames);
        if matches!(result, Err(HostError::ProcessDead)) {
            if let Some(dead) = self.instances.remove(&id) {
                eprintln!(
                    "[maolan-host-adapter] プラグインプロセス異常終了検知: id={} shm={}",
                    id,
                    dead.shm_name()
                );
            }
        }
        result
    }

    pub fn idle(&self) {}
}
