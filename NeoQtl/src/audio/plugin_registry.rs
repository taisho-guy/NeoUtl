use maolan_host_adapter::{PluginCatalogEntry, PluginFormat};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static REGISTRY: OnceLock<Mutex<Vec<PluginCatalogEntry>>> = OnceLock::new();
static DISABLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn registry_lock() -> &'static Mutex<Vec<PluginCatalogEntry>> {
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

fn disabled_lock() -> &'static Mutex<HashSet<String>> {
    DISABLED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn dedup_sorted(mut entries: Vec<PluginCatalogEntry>) -> Vec<PluginCatalogEntry> {
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        let key = (entry.format, entry.plugin_id.clone(), entry.path.clone());
        seen.insert(key)
    });
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn log_entries(entries: &[PluginCatalogEntry]) {
    for entry in entries {
        eprintln!(
            "{}",
            t!(
                "[NeoUtl] audioプラグイン登録: %{arg0} (%{arg1}, %{arg2})",
                arg0 = format!("{}", entry.name),
                arg1 = format!("{:?}", entry.format),
                arg2 = format!("{}", entry.path.display())
            )
        );
    }
}

pub fn init_from_cache(entries: Vec<PluginCatalogEntry>, disabled_ids: &[String]) {
    let entries = dedup_sorted(entries);
    log_entries(&entries);
    *registry_lock().lock().unwrap() = entries;
    *disabled_lock().lock().unwrap() = disabled_ids.iter().cloned().collect();
}

pub fn set_disabled(disabled_ids: &[String]) {
    *disabled_lock().lock().unwrap() = disabled_ids.iter().cloned().collect();
}

pub fn rescan(paths: &[PathBuf], auto_detect_system: bool) -> Vec<PluginCatalogEntry> {
    let mut entries = Vec::new();
    entries.extend(maolan_host_adapter::scan_directories(
        PluginFormat::Vst3,
        paths,
    ));
    entries.extend(maolan_host_adapter::scan_directories(
        PluginFormat::Clap,
        paths,
    ));
    #[cfg(unix)]
    entries.extend(maolan_host_adapter::scan_directories(
        PluginFormat::Lv2,
        paths,
    ));

    if auto_detect_system {
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Vst3));
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Clap));
        #[cfg(unix)]
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Lv2));
    }

    let entries = dedup_sorted(entries);
    log_entries(&entries);
    *registry_lock().lock().unwrap() = entries.clone();
    entries
}

pub fn get_all() -> Vec<PluginCatalogEntry> {
    let disabled = disabled_lock().lock().unwrap();
    registry_lock()
        .lock()
        .unwrap()
        .iter()
        .filter(|e| !disabled.contains(&e.plugin_id))
        .cloned()
        .collect()
}

pub fn get_all_unfiltered() -> Vec<PluginCatalogEntry> {
    registry_lock().lock().unwrap().clone()
}

pub fn is_disabled(plugin_id: &str) -> bool {
    disabled_lock().lock().unwrap().contains(plugin_id)
}

pub fn find_by_id_or_path(id_or_path: &str) -> Option<PluginCatalogEntry> {
    get_all()
        .into_iter()
        .find(|e| e.plugin_id == id_or_path || e.path.to_string_lossy() == id_or_path)
}

pub fn default_plugins_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("audio-plugins");
    };
    exe_dir.join("audio-plugins")
}
