use crate::PluginFormat;
use clack_host::factory::plugin::PluginFactory;
use clack_host::prelude::PluginEntry;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::path::{Path, PathBuf};

/// エフェクトカタログのCatalogRowへ直接写像可能な形へ正規化した1プラグイン分の情報。
/// PluginChain::pushはこのpath/plugin_idのペアでプラグイン実体をロードする。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCatalogEntry {
    pub format: PluginFormat,
    pub path: PathBuf,
    pub plugin_id: String,
    pub name: String,
    pub vendor: String,
}

/// dir配下のVST3バンドルを走査する。vst3-hostのビルトインスキャナ
/// （モジュール単位でmoduleinfo.json優先、なければCOM経由の実ロードで内省）に委譲する。
pub fn discover_vst3(dir: &Path) -> Vec<PluginCatalogEntry> {
    let infos = match vst3_host::simple::discover_plugins_in(dir) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("[NeoUtl] vst3 discover 失敗 {}: {err}", dir.display());
            return Vec::new();
        }
    };
    infos
        .into_iter()
        .map(|info| PluginCatalogEntry {
            format: PluginFormat::Vst3,
            path: info.path,
            plugin_id: info.uid,
            name: info.name,
            vendor: info.vendor,
        })
        .collect()
}

/// dir配下の`.clap`ファイルを走査する。ファイル単位でPluginEntryを一時ロードし、
/// PluginFactory::plugin_descriptorsから列挙後、直ちにEntryを破棄する
/// （実インスタンス化はPluginChainへ追加された時点まで遅延させる方針のため、
/// ここではメタデータ取得のみに限定する）。
pub fn discover_clap(dir: &Path) -> Vec<PluginCatalogEntry> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[NeoUtl] clap discover 失敗 {}: {err}", dir.display());
            return Vec::new();
        }
    };

    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("clap"))
        .flat_map(|path| discover_clap_file(&path))
        .collect()
}

fn discover_clap_file(path: &Path) -> Vec<PluginCatalogEntry> {
    let Some(path_str) = path.to_str() else {
        eprintln!("[NeoUtl] clap discover: non-UTF8 path {}", path.display());
        return Vec::new();
    };

    let entry = match unsafe { PluginEntry::load(path_str) } {
        Ok(e) => e,
        Err(err) => {
            eprintln!("[NeoUtl] clap discover 読込失敗 {}: {err}", path.display());
            return Vec::new();
        }
    };

    let Some(factory) = entry.get_factory::<PluginFactory>() else {
        eprintln!(
            "[NeoUtl] clap discover: PluginFactory未提供 {}",
            path.display()
        );
        return Vec::new();
    };

    (0..factory.plugin_count())
        .filter_map(|i| factory.plugin_descriptor(i))
        .filter_map(|desc| {
            let id = desc.id()?.to_str().ok()?.to_owned();
            let name = desc.name()?.to_str().ok()?.to_owned();
            let vendor = desc
                .vendor()
                .and_then(|c| c.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            Some(PluginCatalogEntry {
                format: PluginFormat::Clap,
                path: path.to_path_buf(),
                plugin_id: id,
                name,
                vendor,
            })
        })
        .collect()
}

/// CLAP factoryの`plugin_id`をロード時に渡すためのCString化。
/// PluginId文字列はNUL終端要件（CLAP規約）を満たす前提だが、
/// UI経由の値には保証がないため、ここで検証を1箇所へ集約する。
pub fn clap_plugin_id_cstring(plugin_id: &str) -> Result<CString, crate::PluginError> {
    CString::new(plugin_id)
        .map_err(|e| crate::PluginError::Clap(format!("plugin_id contains NUL: {e}")))
}
