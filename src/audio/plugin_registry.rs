use neoutl_audio_plugin_host::{
    PluginCatalogEntry, PluginFormat, discover_clap_file, discover_clap_paths, discover_vst3_file,
    discover_vst3_paths,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// effects/loader.rsと同型のRegistry。discover結果を起動時に一度だけ確定し、
/// UIのカタログ表示はここから読み取る。実プラグイン読込（load_vst3/load_clap）は
/// PluginChainへ追加された時点までAudioEngine側で遅延実行する。
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
                "[NeoUtl] audioプラグイン登録: {} ({:?}, {})",
                entry.name, entry.format, entry.plugin_id
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

pub fn registry() -> &'static [PluginCatalogEntry] {
    REGISTRY.get().map_or(&[][..], Vec::as_slice)
}

pub fn by_plugin_id(plugin_id: &str) -> Option<&'static PluginCatalogEntry> {
    registry().iter().find(|e| e.plugin_id == plugin_id)
}

pub fn by_path(path: &str) -> Option<&'static PluginCatalogEntry> {
    registry().iter().find(|e| e.path.to_string_lossy() == path)
}

pub fn resolve(path: &Path, format: PluginFormat) -> Option<PluginCatalogEntry> {
    let entries = match format {
        PluginFormat::Vst3 => discover_vst3_file(path),
        PluginFormat::Clap => discover_clap_file(path),
    };
    entries.into_iter().next()
}

/// effects::loader::default_effects_dirと同型。VST3/CLAP双方を1ディレクトリへ集約する
/// 前提とし、macOSのResourcesバンドル配下も同様に優先探索する。
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
