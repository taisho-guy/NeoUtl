use crate::ecs::resources::AudioPluginSettingsResource;
use std::path::PathBuf;

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent()
                .map(|d| d.join("settings").join("audio-plugin-settings.npb"))
        })
        .unwrap_or_else(|| PathBuf::from("settings/audio-plugin-settings.npb"))
}

pub fn save_to_disk(s: &AudioPluginSettingsResource) -> std::io::Result<()> {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let encoded = crate::schema::encode_schema(s);
    std::fs::write(path, encoded)
}

pub fn load_from_disk() -> Option<AudioPluginSettingsResource> {
    let bytes = std::fs::read(settings_path()).ok()?;
    crate::schema::decode_schema::<AudioPluginSettingsResource>(&bytes).ok()
}
