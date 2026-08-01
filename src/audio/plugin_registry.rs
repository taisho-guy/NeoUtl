use neoutl_audio_plugin_host::{PluginCatalogEntry, discover_clap, discover_vst3};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// effects/loader.rsと同型のRegistry。discover結果を起動時に一度だけ確定し、
/// UIのカタログ表示はここから読み取る。実プラグイン読込（load_vst3/load_clap）は
/// PluginChainへ追加された時点までAudioEngine側で遅延実行する。
static REGISTRY: OnceLock<Vec<PluginCatalogEntry>> = OnceLock::new();

pub fn load_all(plugins_dir: &Path) {
    REGISTRY.get_or_init(|| {
        let mut entries = discover_vst3(plugins_dir);
        entries.extend(discover_clap(plugins_dir));
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

pub fn registry() -> &'static [PluginCatalogEntry] {
    REGISTRY.get().map_or(&[][..], Vec::as_slice)
}

pub fn by_plugin_id(plugin_id: &str) -> Option<&'static PluginCatalogEntry> {
    registry().iter().find(|e| e.plugin_id == plugin_id)
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
