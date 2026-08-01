use neoutl_audio_plugin_host::PluginFormat;
use serde::{Deserialize, Serialize};
use shipyard::Component;
use std::collections::HashMap;
use std::path::PathBuf;

/// PluginChainが保持する1件分のメタデータ。実体（Box<dyn NeoPlugin>）はここに含めない
/// （ShipyardのComponentはSend + 'staticかつ複製・シリアライズ対象となるため、
/// COMハンドル等を内包する実体をそのまま持たせない）。実体はAudioEngine
/// （src/audio/mixer.rs管轄）側でentity idキーのマップとして別途保持する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInstanceRef {
    pub format: PluginFormat,
    pub path: PathBuf,
    pub plugin_id: String,
    pub bypass: bool,
    /// パラメータID→正規化値(0.0..=1.0、VST3のParameter::value空間に合わせる)。
    pub params: HashMap<u32, f64>,
}

impl PluginInstanceRef {
    pub fn new(format: PluginFormat, path: PathBuf, plugin_id: String) -> Self {
        Self {
            format,
            path,
            plugin_id,
            bypass: false,
            params: HashMap::new(),
        }
    }
}

/// audioオブジェクトに付随するプラグイン（VST3/CLAP）の順序付きチェーン。
/// EffectStackの音声版に相当し、永続化・UI操作の骨格を同一に保つ。
#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct PluginChain(pub Vec<PluginInstanceRef>);

impl PluginChain {
    pub fn push(&mut self, format: PluginFormat, path: PathBuf, plugin_id: String) {
        self.0.push(PluginInstanceRef::new(format, path, plugin_id));
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.0.len() {
            self.0.remove(index);
        }
    }

    pub fn reorder(&mut self, from: usize, to: usize) {
        if from < self.0.len() && to < self.0.len() {
            let item = self.0.remove(from);
            self.0.insert(to, item);
        }
    }

    pub fn set_bypass(&mut self, index: usize, bypass: bool) {
        if let Some(entry) = self.0.get_mut(index) {
            entry.bypass = bypass;
        }
    }

    pub fn set_param(&mut self, index: usize, param_id: u32, value: f64) {
        if let Some(entry) = self.0.get_mut(index) {
            entry.params.insert(param_id, value);
        }
    }
}
