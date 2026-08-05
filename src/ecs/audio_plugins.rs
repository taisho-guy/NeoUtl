use neoutl_audio_plugin_host::{PluginFormat, PluginParamInfo};
use serde::{Deserialize, Serialize};
use shipyard::Component;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PLUGIN_INSTANCE_UID: AtomicU64 = AtomicU64::new(1);

fn next_plugin_instance_uid() -> u64 {
    NEXT_PLUGIN_INSTANCE_UID.fetch_add(1, Ordering::Relaxed)
}

/// PluginChainが保持する1件分のメタデータ。実体（Box<dyn NeoPlugin>）はここに含めない
/// （ShipyardのComponentはSend + 'staticかつ複製・シリアライズ対象となるため、
/// COMハンドル等を内包する実体をそのまま持たせない）。実体はAudioEngine
/// （src/audio/mixer.rs管轄）側でentity idキーのマップとして別途保持する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInstanceRef {
    #[serde(default)]
    pub instance_uid: u64,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub plugin_id: String,
    pub bypass: bool,
    /// パラメータID→正規化値(0.0..=1.0、VST3のParameter::value空間に合わせる)。
    pub params: HashMap<u32, f64>,
    #[serde(default)]
    pub param_info: Vec<PluginParamInfo>,
}

/// audioオブジェクトに付随するプラグイン（VST3/CLAP）の順序付きチェーン。
/// EffectStackの音声版に相当し、永続化・UI操作の骨格を同一に保つ。
#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct PluginChain(pub Vec<PluginInstanceRef>);

impl PluginChain {
    pub fn repair_instance_uids(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for instance in &mut self.0 {
            if instance.instance_uid == 0 || !seen.insert(instance.instance_uid) {
                instance.instance_uid = next_plugin_instance_uid();
                seen.insert(instance.instance_uid);
            } else {
                let current = NEXT_PLUGIN_INSTANCE_UID.load(Ordering::Relaxed);
                if instance.instance_uid >= current {
                    let _ = NEXT_PLUGIN_INSTANCE_UID.compare_exchange(
                        current,
                        instance.instance_uid.saturating_add(1),
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    );
                }
            }
        }
    }
}
