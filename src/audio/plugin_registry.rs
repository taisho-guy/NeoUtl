use carla_host_sys::{
    PluginCatalogEntry, PluginFormat, discover_clap_paths, discover_lv2_paths, discover_vst3_paths,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Vec<PluginCatalogEntry>> = OnceLock::new();

pub fn load_all(plugins_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut entries = Vec::new();

        let internal_count = carla_host_sys::get_cached_plugin_count(PluginFormat::Internal, None);
        for i in 0..internal_count {
            if let Some(info) = carla_host_sys::get_cached_plugin_info(PluginFormat::Internal, i) {
                if !info.name.is_empty() {
                    entries.push(PluginCatalogEntry {
                        format: PluginFormat::Internal,
                        name: info.name,
                        vendor: if info.maker.is_empty() {
                            "Carla".to_string()
                        } else {
                            info.maker
                        },
                        plugin_id: info.label,
                        path: PathBuf::new(),
                    });
                }
            }
        }

        let lv2_count = carla_host_sys::get_cached_plugin_count(PluginFormat::Lv2, None);
        for i in 0..lv2_count {
            if let Some(info) = carla_host_sys::get_cached_plugin_info(PluginFormat::Lv2, i) {
                let (bundle_path, uri) = if let Some(pos) = info.label.find("/http") {
                    (&info.label[..pos], &info.label[pos + 1..])
                } else if let Some(pos) = info.label.find("://") {
                    if let Some(slash) = info.label[..pos].rfind('/') {
                        (&info.label[..slash], &info.label[slash + 1..])
                    } else {
                        ("", info.label.as_str())
                    }
                } else if let Some(pos) = info.label.find('/') {
                    (&info.label[..pos], &info.label[pos + 1..])
                } else {
                    ("", info.label.as_str())
                };

                let path = if bundle_path.is_empty() {
                    PathBuf::new()
                } else {
                    PathBuf::from(bundle_path)
                };

                entries.push(PluginCatalogEntry {
                    format: PluginFormat::Lv2,
                    name: info.name,
                    vendor: info.maker,
                    plugin_id: uri.to_string(),
                    path,
                });
            }
        }

        if lv2_count == 0 {
            let mut lv2_roots = system_lv2_dirs();
            lv2_roots.push(plugins_dir.to_path_buf());
            for root in lv2_roots {
                for path in discover_lv2_paths(&root) {
                    let name = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned();
                    entries.push(PluginCatalogEntry {
                        format: PluginFormat::Lv2,
                        name,
                        vendor: String::new(),
                        plugin_id: path.to_string_lossy().into_owned(),
                        path,
                    });
                }
            }
        }

        let mut vst3_roots = system_vst3_dirs();
        vst3_roots.push(plugins_dir.to_path_buf());
        for root in vst3_roots {
            for path in discover_vst3_paths(&root) {
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_owned();
                let vendor = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .filter(|&v| v != "vst3" && v != "VST3" && v != "yabridge")
                    .unwrap_or_default()
                    .to_owned();
                entries.push(PluginCatalogEntry {
                    format: PluginFormat::Vst3,
                    name,
                    vendor,
                    plugin_id: path.to_string_lossy().into_owned(),
                    path,
                });
            }
        }

        let mut clap_roots = system_clap_dirs();
        clap_roots.push(plugins_dir.to_path_buf());
        for root in clap_roots {
            for path in discover_clap_paths(&root) {
                let name = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_owned();
                entries.push(PluginCatalogEntry {
                    format: PluginFormat::Clap,
                    name,
                    vendor: String::new(),
                    plugin_id: path.to_string_lossy().into_owned(),
                    path,
                });
            }
        }

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

fn add_env_paths(roots: &mut Vec<PathBuf>, variable: &str) {
    if let Some(value) = std::env::var_os(variable) {
        roots.extend(std::env::split_paths(&value));
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn system_vst3_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    add_env_paths(&mut roots, "VST3_PATH");
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join(".vst3"));
        }
        roots.extend([
            PathBuf::from("/usr/lib/vst3"),
            PathBuf::from("/usr/local/lib/vst3"),
        ]);
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join("Library/Audio/Plug-Ins/VST3"));
        }
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            roots.push(PathBuf::from(common).join("VST3"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs/Common/VST3"));
        }
    }
    roots
}

fn system_clap_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    add_env_paths(&mut roots, "CLAP_PATH");
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join(".clap"));
        }
        roots.extend([
            PathBuf::from("/usr/lib/clap"),
            PathBuf::from("/usr/local/lib/clap"),
        ]);
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        }
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local).join("Programs/Common/CLAP"));
        }
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            roots.push(PathBuf::from(common).join("CLAP"));
        }
    }
    roots
}

fn system_lv2_dirs() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    add_env_paths(&mut roots, "LV2_PATH");
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join(".lv2"));
        }
        roots.extend([
            PathBuf::from("/usr/lib/lv2"),
            PathBuf::from("/usr/local/lib/lv2"),
        ]);
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            roots.push(home.join("Library/Audio/Plug-Ins/LV2"));
        }
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/LV2"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            roots.push(PathBuf::from(common).join("LV2"));
        }
    }
    roots
}

pub fn default_plugins_dir() -> PathBuf {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from("audio-plugins");
    };

    #[cfg(target_os = "macos")]
    {
        let resources_dir = exe_dir.join("../Resources/audio-plugins");
        if resources_dir.is_dir() {
            return resources_dir;
        }
    }

    exe_dir.join("audio-plugins")
}
