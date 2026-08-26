use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CRASH_THRESHOLD: u32 = 3;

#[derive(serde::Serialize, serde::Deserialize)]
struct BlocklistEntry {
    path: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    timestamp: String,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct BlocklistFile {
    entries: Vec<BlocklistEntry>,
}

struct CrashState {
    counts: HashMap<String, u32>,
    blocked: HashSet<String>,
}

fn state() -> &'static Mutex<CrashState> {
    static STATE: OnceLock<Mutex<CrashState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(CrashState {
            counts: HashMap::new(),
            blocked: load_persistent_blocked(),
        })
    })
}

fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(unix)]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

fn blocklist_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("maolan").join("plugin-blocklist.json"))
}

fn load_blocklist_file() -> BlocklistFile {
    let Some(path) = blocklist_path() else {
        return BlocklistFile::default();
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn load_persistent_blocked() -> HashSet<String> {
    load_blocklist_file()
        .entries
        .into_iter()
        .map(|entry| entry.path)
        .collect()
}

pub fn record_crash(plugin_spec: &str) -> u32 {
    let mut guard = state().lock().unwrap();
    let count = guard.counts.entry(plugin_spec.to_string()).or_insert(0);
    *count += 1;
    *count
}

pub fn is_blocked(plugin_spec: &str) -> bool {
    state().lock().unwrap().blocked.contains(plugin_spec)
}

pub fn block_plugin(plugin_spec: &str, reason: &str) {
    {
        let mut guard = state().lock().unwrap();
        guard.blocked.insert(plugin_spec.to_string());
    }
    let Some(path) = blocklist_path() else {
        return;
    };
    let mut file = load_blocklist_file();
    if file.entries.iter().any(|entry| entry.path == plugin_spec) {
        return;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    file.entries.push(BlocklistEntry {
        path: plugin_spec.to_string(),
        error: reason.to_string(),
        timestamp,
    });
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(&file) else {
        return;
    };
    if let Ok(mut handle) = std::fs::File::create(&path) {
        let _ = handle.write_all(json.as_bytes());
    }
}
