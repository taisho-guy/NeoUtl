use std::path::PathBuf;

pub fn default_binary_path() -> PathBuf {
    let exe_name = if cfg!(windows) {
        "maolan-plugin-host.exe"
    } else {
        "maolan-plugin-host"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe_name)))
        .unwrap_or_else(|| PathBuf::from(exe_name))
}
