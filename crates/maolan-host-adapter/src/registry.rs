use crate::types::{PluginCatalogEntry, PluginFormat};
use std::sync::{Mutex, OnceLock};

struct ScanCache {
    clap: Option<Vec<PluginCatalogEntry>>,
    vst3: Option<Vec<PluginCatalogEntry>>,
    lv2: Option<Vec<PluginCatalogEntry>>,
}

fn cache() -> &'static Mutex<ScanCache> {
    static CACHE: OnceLock<Mutex<ScanCache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(ScanCache {
            clap: None,
            vst3: None,
            lv2: None,
        })
    })
}

pub fn invalidate(format: PluginFormat) {
    let mut c = cache().lock().unwrap();
    match format {
        PluginFormat::Clap => c.clap = None,
        PluginFormat::Vst3 => c.vst3 = None,
        PluginFormat::Lv2 => c.lv2 = None,
        _ => {}
    }
}

pub fn catalog(format: PluginFormat) -> Vec<PluginCatalogEntry> {
    let mut c = cache().lock().unwrap();
    match format {
        PluginFormat::Clap => {
            if c.clap.is_none() {
                c.clap = Some(
                    maolan_plugin_host::scan::scan_clap_plugins(false)
                        .into_iter()
                        .map(|p| PluginCatalogEntry {
                            format: PluginFormat::Clap,
                            name: p.name,
                            vendor: String::new(),
                            plugin_id: p.id,
                            path: std::path::PathBuf::from(p.path),
                        })
                        .collect(),
                );
            }
            c.clap.clone().unwrap_or_default()
        }
        PluginFormat::Vst3 => {
            if c.vst3.is_none() {
                c.vst3 = Some(
                    maolan_plugin_host::scan::scan_vst3_plugins()
                        .into_iter()
                        .map(|p| PluginCatalogEntry {
                            format: PluginFormat::Vst3,
                            name: p.name,
                            vendor: p.vendor,
                            plugin_id: p.id,
                            path: std::path::PathBuf::from(p.path),
                        })
                        .collect(),
                );
            }
            c.vst3.clone().unwrap_or_default()
        }
        #[cfg(unix)]
        PluginFormat::Lv2 => {
            if c.lv2.is_none() {
                c.lv2 = Some(
                    maolan_plugin_host::scan::scan_lv2_plugins()
                        .into_iter()
                        .map(|p| PluginCatalogEntry {
                            format: PluginFormat::Lv2,
                            name: p.name,
                            vendor: String::new(),
                            plugin_id: p.uri,
                            path: std::path::PathBuf::from(p.bundle_uri),
                        })
                        .collect(),
                );
            }
            c.lv2.clone().unwrap_or_default()
        }
        _ => Vec::new(),
    }
}
