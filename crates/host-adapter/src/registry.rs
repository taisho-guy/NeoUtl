use crate::binary_path::default_binary_path;
use crate::types::{PluginCatalogEntry, PluginFormat};
use maolan_plugin_host::blocklist::{Blocklist, BlocklistEntry};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const VST3_CRASH_RETRY_LIMIT: u32 = 32;

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
                c.clap = Some(scan_system("clap"));
            }
            c.clap.clone().unwrap_or_default()
        }
        PluginFormat::Vst3 => {
            if c.vst3.is_none() {
                c.vst3 = Some(scan_system("vst3"));
            }
            c.vst3.clone().unwrap_or_default()
        }
        #[cfg(unix)]
        PluginFormat::Lv2 => {
            if c.lv2.is_none() {
                c.lv2 = Some(scan_system("lv2"));
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

fn scan_output_path(format_tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "neoutl-plugin-scan-{}-{}-{}.json",
        std::process::id(),
        format_tag,
        Instant::now().elapsed().as_nanos()
    ))
}

fn scan_system(format_tag: &str) -> Vec<PluginCatalogEntry> {
    match format_tag {
        "clap" => scan_clap_system(),
        "vst3" => scan_vst3_system(),
        "lv2" => scan_lv2_system(),
        _ => Vec::new(),
    }
}

fn blocklist_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("maolan")
        .join("plugin-blocklist.json")
}

fn unix_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn block_vst3_bundle(bundle_path: &str, error: &str) {
    let path = blocklist_path();
    let mut list = Blocklist::load_from(&path);
    if list.is_blocked(bundle_path) {
        return;
    }
    list.entries.push(BlocklistEntry {
        path: bundle_path.to_string(),
        error: error.to_string(),
        timestamp: unix_timestamp(),
    });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&list) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!(
                    "[maolan-host-adapter] ブロックリスト書込失敗 path={}: {e}",
                    path.display()
                );
            }
        }
        Err(e) => {
            eprintln!("[maolan-host-adapter] ブロックリスト直列化失敗: {e}");
        }
    }
}

fn last_scanning_vst3_bundle(stderr: &str) -> Option<String> {
    const MARKER: &str = "scanning VST3 plugin bundle: ";
    let start = stderr.rfind(MARKER)? + MARKER.len();
    let rest = &stderr[start..];
    let end = rest.find('\n').unwrap_or(rest.len());
    let path = rest[..end].trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

fn run_scan_process_ex(
    format_tag: &str,
    extra_args: &[&str],
    capture_stderr: bool,
) -> (Option<String>, Option<ExitStatus>, String) {
    let out_path = scan_output_path(format_tag);
    let _ = std::fs::remove_file(&out_path);

    eprintln!("[maolan-host-adapter] プラグイン走査開始 format={format_tag} path=--system");

    let mut cmd = Command::new(default_binary_path());
    cmd.arg("--scan")
        .arg("--format")
        .arg(format_tag)
        .arg("--path")
        .arg("--system")
        .arg("--output")
        .arg(&out_path);
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null());
    cmd.stderr(if capture_stderr {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "[maolan-host-adapter] プラグイン走査プロセス起動失敗 format={format_tag}: {e}"
            );
            return (None, None, String::new());
        }
    };

    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = String::new();
            let _ = pipe.read_to_string(&mut buf);
            buf
        })
    });

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

    let captured_stderr = stderr_reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    if !capture_stderr {
    } else {
        eprint!("{captured_stderr}");
    }

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
    (result, final_status, captured_stderr)
}

fn run_scan_process(format_tag: &str) -> Option<String> {
    run_scan_process_ex(format_tag, &[], false).0
}

fn scan_clap_system() -> Vec<PluginCatalogEntry> {
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
            path: PathBuf::from(p.path),
        })
        .collect()
}

fn parse_vst3_scan_json(json: &str) -> Option<Vec<PluginCatalogEntry>> {
    let parsed = serde_json::from_str::<ScanOutput<Vec<ScanVst3Entry>>>(json).ok()?;
    Some(
        parsed
            .data
            .into_iter()
            .map(|p| PluginCatalogEntry {
                format: PluginFormat::Vst3,
                name: p.name,
                vendor: p.vendor,
                plugin_id: p.id,
                path: PathBuf::from(p.path),
            })
            .collect(),
    )
}

fn scan_vst3_system() -> Vec<PluginCatalogEntry> {
    for attempt in 1..=VST3_CRASH_RETRY_LIMIT {
        let (json, status, stderr) = run_scan_process_ex("vst3", &["--log-level", "debug"], true);

        if let Some(json) = json {
            let Some(entries) = parse_vst3_scan_json(&json) else {
                eprintln!("[maolan-host-adapter] プラグイン走査結果解析失敗 format=vst3");
                return Vec::new();
            };
            return entries;
        }

        let Some(status) = status else {
            return Vec::new();
        };
        if status.success() {
            return Vec::new();
        }

        let Some(crash_path) = last_scanning_vst3_bundle(&stderr) else {
            eprintln!(
                "[maolan-host-adapter] VST3走査クラッシュ原因バンドル特定失敗 status={status:?} attempt={attempt}"
            );
            return Vec::new();
        };

        eprintln!(
            "[maolan-host-adapter] VST3プラグインクラッシュ検出 path={crash_path} status={status:?} attempt={attempt} ブロックリスト追加後再試行"
        );
        block_vst3_bundle(&crash_path, &format!("scan crash status={status:?}"));
    }

    eprintln!(
        "[maolan-host-adapter] VST3走査再試行上限到達 limit={VST3_CRASH_RETRY_LIMIT} 走査中断"
    );
    Vec::new()
}

#[cfg(unix)]
fn scan_lv2_system() -> Vec<PluginCatalogEntry> {
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
            path: PathBuf::from(p.bundle_uri),
        })
        .collect()
}

#[cfg(not(unix))]
fn scan_lv2_system() -> Vec<PluginCatalogEntry> {
    Vec::new()
}

fn format_extension(format: PluginFormat) -> &'static str {
    match format {
        PluginFormat::Vst3 => "vst3",
        PluginFormat::Clap => "clap",
        PluginFormat::Lv2 => "lv2",
        _ => "",
    }
}

fn plugin_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn collect_from_dir(
    format: PluginFormat,
    dir: &Path,
    ext: &str,
    out: &mut Vec<PluginCatalogEntry>,
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let is_match = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext));
        if is_match {
            out.push(PluginCatalogEntry {
                format,
                name: plugin_name_from_path(&path),
                vendor: String::new(),
                plugin_id: path.to_string_lossy().to_string(),
                path,
            });
            continue;
        }
        if path.is_dir() {
            collect_from_dir(format, &path, ext, out);
        }
    }
}

pub fn scan_directories(format: PluginFormat, dirs: &[PathBuf]) -> Vec<PluginCatalogEntry> {
    let ext = format_extension(format);
    if ext.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for dir in dirs {
        collect_from_dir(format, dir, ext, &mut out);
    }
    out
}
