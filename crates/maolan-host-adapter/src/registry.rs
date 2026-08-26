use crate::binary_path::default_binary_path;
use crate::types::{PluginCatalogEntry, PluginFormat};
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SCAN_TIMEOUT: Duration = Duration::from_secs(20);

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
                c.clap = Some(scan_clap());
            }
            c.clap.clone().unwrap_or_default()
        }
        PluginFormat::Vst3 => {
            if c.vst3.is_none() {
                c.vst3 = Some(scan_vst3());
            }
            c.vst3.clone().unwrap_or_default()
        }
        #[cfg(unix)]
        PluginFormat::Lv2 => {
            if c.lv2.is_none() {
                c.lv2 = Some(scan_lv2());
            }
            c.lv2.clone().unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

#[derive(Deserialize)]
struct ScanOutput<T> {
    data: T,
}

#[derive(Deserialize)]
struct ScanClapEntry {
    id: String,
    name: String,
    path: String,
}

#[derive(Deserialize)]
struct ScanVst3Entry {
    id: String,
    name: String,
    vendor: String,
    path: String,
}

#[cfg(unix)]
#[derive(Deserialize)]
struct ScanLv2Entry {
    uri: String,
    name: String,
    bundle_uri: String,
}

fn scan_output_path(format_tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "neoutl-plugin-scan-{}-{}-{}.json",
        std::process::id(),
        format_tag,
        Instant::now().elapsed().as_nanos()
    ))
}

fn run_scan_process(format_tag: &str) -> Option<String> {
    let out_path = scan_output_path(format_tag);
    let _ = std::fs::remove_file(&out_path);

    eprintln!("[maolan-host-adapter] プラグイン走査開始 format={format_tag}");

    let mut child = match Command::new(default_binary_path())
        .arg("--scan")
        .arg("--format")
        .arg(format_tag)
        .arg("--path")
        .arg("--system")
        .arg("--output")
        .arg(&out_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[maolan-host-adapter] プラグイン走査プロセス起動失敗 format={format_tag}: {e}"
            );
            return None;
        }
    };

    let deadline = Instant::now() + SCAN_TIMEOUT;
    let final_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    eprintln!(
                        "[maolan-host-adapter] プラグイン走査タイムアウト format={format_tag} timeout={}秒 強制終了実行",
                        SCAN_TIMEOUT.as_secs()
                    );
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("[maolan-host-adapter] 走査プロセス強制終了完了 format={format_tag}");
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!(
                    "[maolan-host-adapter] プラグイン走査プロセス監視失敗 format={format_tag}: {e}"
                );
                break None;
            }
        }
    };

    let result = match final_status {
        Some(status) if status.success() => {
            eprintln!("[maolan-host-adapter] プラグイン走査完了 format={format_tag}");
            std::fs::read_to_string(&out_path).ok()
        }
        Some(status) => {
            eprintln!(
                "[maolan-host-adapter] プラグイン走査プロセス異常終了 format={format_tag} status={status:?}"
            );
            None
        }
        None => None,
    };

    let _ = std::fs::remove_file(&out_path);
    result
}

fn scan_clap() -> Vec<PluginCatalogEntry> {
    let Some(json) = run_scan_process("clap") else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<ScanOutput<Vec<ScanClapEntry>>>(&json) else {
        eprintln!("[maolan-host-adapter] プラグイン走査結果解析失敗 format=clap");
        return Vec::new();
    };
    parsed
        .data
        .into_iter()
        .map(|p| PluginCatalogEntry {
            format: PluginFormat::Clap,
            name: p.name,
            vendor: String::new(),
            plugin_id: p.id,
            path: std::path::PathBuf::from(p.path),
        })
        .collect()
}

fn scan_vst3() -> Vec<PluginCatalogEntry> {
    let Some(json) = run_scan_process("vst3") else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<ScanOutput<Vec<ScanVst3Entry>>>(&json) else {
        eprintln!("[maolan-host-adapter] プラグイン走査結果解析失敗 format=vst3");
        return Vec::new();
    };
    parsed
        .data
        .into_iter()
        .map(|p| PluginCatalogEntry {
            format: PluginFormat::Vst3,
            name: p.name,
            vendor: p.vendor,
            plugin_id: p.id,
            path: std::path::PathBuf::from(p.path),
        })
        .collect()
}

#[cfg(unix)]
fn scan_lv2() -> Vec<PluginCatalogEntry> {
    let Some(json) = run_scan_process("lv2") else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<ScanOutput<Vec<ScanLv2Entry>>>(&json) else {
        eprintln!("[maolan-host-adapter] プラグイン走査結果解析失敗 format=lv2");
        return Vec::new();
    };
    parsed
        .data
        .into_iter()
        .map(|p| PluginCatalogEntry {
            format: PluginFormat::Lv2,
            name: p.name,
            vendor: String::new(),
            plugin_id: p.uri,
            path: std::path::PathBuf::from(p.bundle_uri),
        })
        .collect()
}
