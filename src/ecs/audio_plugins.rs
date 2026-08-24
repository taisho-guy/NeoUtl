use carla_host_sys::{PluginFormat, PluginParamInfo};
use shipyard::Component;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PLUGIN_INSTANCE_UID: AtomicU64 = AtomicU64::new(1);

fn next_plugin_instance_uid() -> u64 {
    NEXT_PLUGIN_INSTANCE_UID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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

impl From<&PluginInstanceRef> for neoutl_schema::PluginInstanceRef {
    fn from(value: &PluginInstanceRef) -> Self {
        Self {
            instance_uid: value.instance_uid,
            format: value.format as i32,
            path: value.path.to_string_lossy().to_string(),
            plugin_id: value.plugin_id.clone(),
            bypass: value.bypass,
            params: value.params.clone(),
            param_info: value
                .param_info
                .iter()
                .map(|info| serde_json::to_vec(info).unwrap_or_default())
                .collect(),
        }
    }
}

impl TryFrom<&neoutl_schema::PluginInstanceRef> for PluginInstanceRef {
    type Error = String;

    fn try_from(value: &neoutl_schema::PluginInstanceRef) -> Result<Self, Self::Error> {
        let format = match value.format {
            x if x == 0 => PluginFormat::Vst3,
            x if x == 1 => PluginFormat::Clap,
            x if x == 2 => PluginFormat::Lv2,
            x if x == 3 => PluginFormat::Vst2,
            x if x == 4 => PluginFormat::Au,
            x if x == 5 => PluginFormat::Sf2,
            x if x == 6 => PluginFormat::Sfz,
            x if x == 7 => PluginFormat::Jsfx,
            _ => PluginFormat::Internal,
        };
        Ok(Self {
            instance_uid: value.instance_uid,
            format,
            path: PathBuf::from(&value.path),
            plugin_id: value.plugin_id.clone(),
            bypass: value.bypass,
            params: value.params.clone(),
            param_info: value
                .param_info
                .iter()
                .filter_map(|bytes| serde_json::from_slice(bytes).ok())
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Default, Component)]
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
