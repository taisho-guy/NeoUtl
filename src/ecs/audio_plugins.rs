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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginInstanceRef {
    #[serde(default)]
    pub instance_uid: u64,
    pub format: PluginFormat,
    pub path: PathBuf,
    pub plugin_id: String,
    pub bypass: bool,
    pub params: HashMap<u32, f64>,
    #[serde(default)]
    pub param_info: Vec<PluginParamInfo>,
}

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
