use crate::ecs::resources::SystemSettingsResource;
use std::path::PathBuf;

#[derive(Clone, Default)]
pub(super) enum ScanStatus {
    #[default]
    Idle,
}

pub(super) fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("system-settings.npb"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/system-settings.npb"))
}

pub(super) fn save_to_disk(s: &SystemSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let encoded = crate::schema::encode_schema(s);
    std::fs::write(path, encoded)
}

pub(crate) fn load_from_disk() -> Option<SystemSettingsResource> {
    let bytes = std::fs::read(settings_path()).ok()?;
    crate::schema::decode_schema::<SystemSettingsResource>(&bytes).ok()
}

pub(super) fn easing_engine_ids_and_names() -> (Vec<String>, Vec<String>) {
    let ids = crate::easings::loader::registry()
        .iter()
        .map(|e| e.id.clone())
        .collect();
    let names = crate::easings::loader::registry()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    (ids, names)
}

pub(super) fn index_of(ids: &[String], id: &str) -> i32 {
    ids.iter().position(|i| i == id).map_or(0, |i| i as i32)
}
