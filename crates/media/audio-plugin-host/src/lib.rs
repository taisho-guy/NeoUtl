mod clap;
mod discover;
mod error;
mod vst3;

pub use clap::ClapWrapper;
pub use discover::{
    PluginCatalogEntry, clap_plugin_id_cstring, discover_clap, discover_clap_file,
    discover_clap_paths, discover_vst3, discover_vst3_file, discover_vst3_paths,
};
pub use error::PluginError;
pub use vst3::{PluginParamInfo, Vst3Wrapper};

use serde::{Deserialize, Serialize};
use std::path::Path;

pub trait NeoPlugin: Send {
    fn start(&mut self) -> Result<(), PluginError>;
    fn stop(&mut self) -> Result<(), PluginError>;
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], frames: usize);
    fn set_parameter(&mut self, id: u32, value: f64) -> Result<(), PluginError>;
    /// パラメータメタデータ列挙。CLAP側（clack-extensions params未統合）は
    /// 空配列を返す（第一段階のバイパスのみ対応方針に対応）。
    fn param_info(&self) -> Vec<PluginParamInfo>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginFormat {
    Vst3,
    Clap,
}

impl PluginFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("vst3") => Some(Self::Vst3),
            Some("clap") => Some(Self::Clap),
            _ => None,
        }
    }
}

/// VST3読込。
pub fn load_vst3(path: &Path) -> Result<Vst3Wrapper, PluginError> {
    Vst3Wrapper::load(path)
}

/// CLAP読込。plugin_idはPluginCatalogEntry::plugin_id（factory ID文字列）をそのまま渡す。
pub fn load_clap(path: &Path, plugin_id: &str) -> Result<ClapWrapper, PluginError> {
    let id = discover::clap_plugin_id_cstring(plugin_id)?;
    ClapWrapper::load(path, &id)
}
