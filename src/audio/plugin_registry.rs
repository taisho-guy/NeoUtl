use neoutl_audio_plugin_host::{
    PluginCatalogEntry, PluginFormat, discover_clap_paths, discover_vst3_paths,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static REGISTRY: OnceLock<Vec<PluginCatalogEntry>> = OnceLock::new();

pub fn load_all(plugins_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut entries = Vec::new();
        let mut vst3_roots = system_vst3_dirs();
        vst3_roots.push(plugins_dir.to_path_buf());
        for root in vst3_roots {
            entries.extend(discover_vst3_paths(&root).into_iter().map(|path| {
                PluginCatalogEntry {
                    format: PluginFormat::Vst3,
                    name: path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                    vendor: String::new(),
                    plugin_id: path.to_string_lossy().into_owned(),
                    path,
                }
            }));
        }
        let mut clap_roots = system_clap_dirs();
        clap_roots.push(plugins_dir.to_path_buf());
        for root in clap_roots {
            entries.extend(discover_clap_paths(&root).into_iter().map(|path| {
                PluginCatalogEntry {
                    format: PluginFormat::Clap,
                    name: path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                    vendor: String::new(),
                    plugin_id: path.to_string_lossy().into_owned(),
                    path,
                }
            }));
        }
        let mut seen = HashSet::new();
        entries.retain(|entry| seen.insert(entry.path.clone()));
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
