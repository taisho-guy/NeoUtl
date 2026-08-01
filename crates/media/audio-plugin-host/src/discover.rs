use crate::PluginFormat;
use clack_host::factory::plugin::PluginFactory;
use clack_host::prelude::PluginEntry;
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    let report =
        vst3_host::discovery::discover_plugins_safe(&[dir.to_path_buf()], Duration::from_secs(3));
    if let Some(err) = report.error {
        eprintln!(
            "[NeoUtl] vst3 safe discover 利用不可 {}: {err}",
            dir.display()
        );
        return Vec::new();
    }
    report
        .plugins
        .into_iter()
        .map(|detailed| PluginCatalogEntry {
            format: PluginFormat::Vst3,
            path: detailed.info.path,
            plugin_id: detailed.info.uid,
            name: detailed.info.name,
            vendor: detailed.info.vendor,
        })
        .collect()
}

pub fn discover_vst3_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_dirs(dir, "vst3", &mut paths);
    paths
}

pub fn discover_vst3_file(path: &Path) -> Vec<PluginCatalogEntry> {
    let report =
        vst3_host::discovery::discover_plugins_safe(&[path.to_path_buf()], Duration::from_secs(10));
    report
        .plugins
        .into_iter()
        .map(|detailed| PluginCatalogEntry {
            format: PluginFormat::Vst3,
            path: detailed.info.path,
            plugin_id: detailed.info.uid,
            name: detailed.info.name,
            vendor: detailed.info.vendor,
        })
        .collect()
}

/// dir配下の`.clap`ファイルを走査する。ファイル単位でPluginEntryを一時ロードし、
/// PluginFactory::plugin_descriptorsから列挙後、直ちにEntryを破棄する
/// （実インスタンス化はPluginChainへ追加された時点まで遅延させる方針のため、
/// ここではメタデータ取得のみに限定する）。
pub fn discover_clap(dir: &Path) -> Vec<PluginCatalogEntry> {
    let mut files = Vec::new();
    collect_files(dir, "clap", &mut files);
    files
        .into_iter()
        .flat_map(|path| discover_clap_file_impl(&path))
        .collect()
}

pub fn discover_clap_paths(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(dir, "clap", &mut files);
    files
}

pub fn discover_clap_file(path: &Path) -> Vec<PluginCatalogEntry> {
    discover_clap_file_impl(path)
}

fn collect_dirs(dir: &Path, extension: &str, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                paths.push(path);
            } else {
                collect_dirs(&path, extension, paths);
            }
        }
    }
}

fn collect_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn discover_clap_file_impl(path: &Path) -> Vec<PluginCatalogEntry> {
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
