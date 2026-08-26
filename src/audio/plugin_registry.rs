use maolan_host_adapter::{PluginCatalogEntry, PluginFormat};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Vec<PluginCatalogEntry>> = OnceLock::new();

pub fn load_all(_plugins_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut entries = Vec::new();
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Vst3));
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Clap));
        #[cfg(unix)]
        entries.extend(maolan_host_adapter::catalog(PluginFormat::Lv2));

        let mut seen = HashSet::new();
        entries.retain(|entry| {
            let key = (entry.format, entry.plugin_id.clone(), entry.path.clone());
            seen.insert(key)
        });
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in &entries {
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
        entries
    });
}

pub fn get_all() -> &'static [PluginCatalogEntry] {
    REGISTRY.get().map(|v| v.as_slice()).unwrap_or(&[])
}

pub fn find_by_id_or_path(id_or_path: &str) -> Option<PluginCatalogEntry> {
    get_all()
        .iter()
        .find(|e| e.plugin_id == id_or_path || e.path.to_string_lossy() == id_or_path)
        .cloned()
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
