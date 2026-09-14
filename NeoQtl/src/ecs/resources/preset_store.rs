use serde::{Deserialize, Serialize};
use shipyard::Unique;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetData {
    pub id: String,
    pub effect_uuid: String,
    pub name: String,
    pub params: Vec<(String, f32)>,
    #[serde(default)]
    pub created_at_unix: u64,
}

pub fn default_presets_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                return PathBuf::from(appdata).join("neoutl").join("presets");
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("neoutl").join("presets");
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return PathBuf::from(home)
                    .join(".config")
                    .join("neoutl")
                    .join("presets");
            }
        }
    }

    PathBuf::from(".neoutl").join("presets")
}

#[allow(dead_code)]
fn sanitize_id(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim_matches('_');
    if trimmed.is_empty() {
        "preset".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Clone, Debug, Unique)]
pub struct PresetStore {
    base_dir: PathBuf,
    presets: HashMap<String, Vec<PresetData>>,
}

#[allow(dead_code)]
impl PresetStore {
    pub fn new() -> Self {
        let base_dir = default_presets_dir();
        Self::with_base_dir(base_dir)
    }

    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        let mut store = Self {
            base_dir,
            presets: HashMap::new(),
        };
        let _ = store.reload();
        store
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn reload(&mut self) -> Result<(), std::io::Error> {
        self.presets.clear();
        if !self.base_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.base_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let effect_uuid = match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                let mut list = Vec::new();
                if let Ok(preset_files) = fs::read_dir(&path) {
                    for preset_entry in preset_files.flatten() {
                        let preset_path = preset_entry.path();
                        if preset_path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                            if let Ok(content) = fs::read_to_string(&preset_path) {
                                if let Ok(data) = serde_json::from_str::<PresetData>(&content) {
                                    list.push(data);
                                }
                            }
                        }
                    }
                }
                list.sort_by(|a, b| a.name.cmp(&b.name));
                self.presets.insert(effect_uuid, list);
            }
        }

        Ok(())
    }

    pub fn get_presets_for_effect(&self, effect_uuid: &str) -> &[PresetData] {
        self.presets
            .get(effect_uuid)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn find_preset(&self, effect_uuid: &str, preset_id: &str) -> Option<&PresetData> {
        self.presets
            .get(effect_uuid)?
            .iter()
            .find(|p| p.id == preset_id)
    }

    pub fn save_preset(
        &mut self,
        effect_uuid: &str,
        name: &str,
        params: Vec<(String, f32)>,
    ) -> Result<PresetData, std::io::Error> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let preset_id = format!("{}_{}", sanitize_id(name), timestamp);
        self.save_preset_with_id(effect_uuid, &preset_id, name, params)
    }

    pub fn save_preset_with_id(
        &mut self,
        effect_uuid: &str,
        preset_id: &str,
        name: &str,
        params: Vec<(String, f32)>,
    ) -> Result<PresetData, std::io::Error> {
        let dir = self.base_dir.join(effect_uuid);
        fs::create_dir_all(&dir)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let data = PresetData {
            id: preset_id.to_string(),
            effect_uuid: effect_uuid.to_string(),
            name: name.to_string(),
            params,
            created_at_unix: timestamp,
        };

        let file_path = dir.join(format!("{}.json", preset_id));
        let serialized = serde_json::to_string_pretty(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(file_path, serialized)?;

        let list = self.presets.entry(effect_uuid.to_string()).or_default();
        if let Some(pos) = list.iter().position(|p| p.id == preset_id) {
            list[pos] = data.clone();
        } else {
            list.push(data.clone());
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(data)
    }

    pub fn delete_preset(
        &mut self,
        effect_uuid: &str,
        preset_id: &str,
    ) -> Result<bool, std::io::Error> {
        let file_path = self
            .base_dir
            .join(effect_uuid)
            .join(format!("{}.json", preset_id));

        let existed = if file_path.exists() {
            fs::remove_file(&file_path)?;
            true
        } else {
            false
        };

        if let Some(list) = self.presets.get_mut(effect_uuid) {
            list.retain(|p| p.id != preset_id);
        }

        Ok(existed)
    }

    pub fn apply_to_params(
        &self,
        effect_uuid: &str,
        preset_id: &str,
        param_names: &[&str],
        out_params: &mut [f32],
    ) -> bool {
        let preset = match self.find_preset(effect_uuid, preset_id) {
            Some(p) => p,
            None => return false,
        };

        let mut applied = false;
        for (name, val) in &preset.params {
            if let Some(idx) = param_names.iter().position(|&p| p == name) {
                if idx < out_params.len() {
                    out_params[idx] = *val;
                    applied = true;
                }
            }
        }

        applied
    }
}

impl Default for PresetStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_store_crud_and_apply() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = PresetStore::with_base_dir(temp_dir.path().to_path_buf());

        let effect_uuid = "neoutl.effect.transform";
        assert_eq!(store.get_presets_for_effect(effect_uuid).len(), 0);

        let params = vec![
            ("X".to_string(), 100.0),
            ("Y".to_string(), 200.0),
            ("Scale".to_string(), 1.5),
        ];
        let saved = store
            .save_preset(effect_uuid, "My Preset", params.clone())
            .unwrap();
        assert_eq!(saved.name, "My Preset");
        assert_eq!(saved.effect_uuid, effect_uuid);
        assert_eq!(saved.params.len(), 3);

        let presets = store.get_presets_for_effect(effect_uuid);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, saved.id);

        let reloaded_store = PresetStore::with_base_dir(temp_dir.path().to_path_buf());
        let reloaded_presets = reloaded_store.get_presets_for_effect(effect_uuid);
        assert_eq!(reloaded_presets.len(), 1);
        assert_eq!(reloaded_presets[0].name, "My Preset");
        assert_eq!(reloaded_presets[0].params, params);

        let param_names = ["X", "Y", "Z", "Scale", "Rotation"];
        let mut current_params = [0.0f32; 5];
        let applied =
            store.apply_to_params(effect_uuid, &saved.id, &param_names, &mut current_params);
        assert!(applied);
        assert_eq!(current_params[0], 100.0);
        assert_eq!(current_params[1], 200.0);
        assert_eq!(current_params[2], 0.0);
        assert_eq!(current_params[3], 1.5);
        assert_eq!(current_params[4], 0.0);

        let deleted = store.delete_preset(effect_uuid, &saved.id).unwrap();
        assert!(deleted);
        assert_eq!(store.get_presets_for_effect(effect_uuid).len(), 0);

        let reloaded_store2 = PresetStore::with_base_dir(temp_dir.path().to_path_buf());
        assert_eq!(reloaded_store2.get_presets_for_effect(effect_uuid).len(), 0);
    }

    #[test]
    fn ecs_world_preset_integration_test() {
        use crate::ecs::EcsWorld;
        use shipyard::{Get, UniqueViewMut};

        let mut world = EcsWorld::new();
        let temp_dir = tempfile::tempdir().unwrap();

        world.world.run(|mut store: UniqueViewMut<PresetStore>| {
            *store = PresetStore::with_base_dir(temp_dir.path().to_path_buf());
        });

        let obj_id = world.add_shape_object(0, 0, 100, 0, Default::default());

        let effect_id = "test.effect.color";
        world.world.run(
            |mut stacks: shipyard::ViewMut<crate::ecs::effects::EffectStack>| {
                if let Some(entity) = world.find_entity(obj_id) {
                    if let Ok(mut stack) = (&mut stacks).get(entity) {
                        let mut inst = crate::ecs::types::EffectInstance::new(effect_id);
                        inst.params.insert(
                            "R".to_string(),
                            crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(
                                0.2,
                            )),
                        );
                        inst.params.insert(
                            "G".to_string(),
                            crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(
                                0.8,
                            )),
                        );
                        inst.params.insert(
                            "B".to_string(),
                            crate::ecs::types::EffectParam::new(crate::ecs::types::Value::Number(
                                1.0,
                            )),
                        );
                        stack.0.push(inst);
                    }
                }
            },
        );

        let saved = world
            .save_effect_preset(obj_id, 0, "Bright Cyan")
            .expect("preset should save");
        assert_eq!(saved.name, "Bright Cyan");
        assert_eq!(saved.params.len(), 3);

        let presets = world.get_effect_presets_for(effect_id);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Bright Cyan");

        world.set_effect_param(obj_id, 0, "R", 0.9);
        world.set_effect_param(obj_id, 0, "G", 0.1);
        assert_eq!(world.effect_param_f32(obj_id, 0, "R"), Some(0.9));
        assert_eq!(world.effect_param_f32(obj_id, 0, "G"), Some(0.1));

        let applied = world
            .apply_effect_preset(obj_id, 0, &saved.id)
            .expect("preset should apply");
        assert!(applied);
        assert_eq!(world.effect_param_f32(obj_id, 0, "R"), Some(0.2));
        assert_eq!(world.effect_param_f32(obj_id, 0, "G"), Some(0.8));
        assert_eq!(world.effect_param_f32(obj_id, 0, "B"), Some(1.0));

        let deleted = world
            .delete_effect_preset(effect_id, &saved.id)
            .expect("preset should delete");
        assert!(deleted);
        assert_eq!(world.get_effect_presets_for(effect_id).len(), 0);
    }
}
