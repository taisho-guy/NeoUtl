use crate::ecs::resources::SystemSettingsResource;
use std::path::PathBuf;

pub fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("system-settings.npb"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/system-settings.npb"))
}

pub fn save_to_disk(s: &SystemSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let encoded = crate::schema::encode_schema(s);
    std::fs::write(path, encoded)
}

pub fn load_from_disk() -> Option<SystemSettingsResource> {
    let bytes = std::fs::read(settings_path()).ok()?;
    crate::schema::decode_schema::<SystemSettingsResource>(&bytes).ok()
}
