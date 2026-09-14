use crate::ecs::EcsWorld;
use crate::ecs::effects::{self, EffectStack};
use shipyard::{Get, UniqueView, UniqueViewMut, View, ViewMut};

impl EcsWorld {
    pub fn add_effect(&mut self, object_id: usize, effect_id: &str) {
        if effects::find_effect(effect_id).is_none() {
            return;
        }
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.push(effect_id);
            }
        });
        self.touch();
    }

    pub fn reorder_effect(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity)
                && from < stack.0.len()
                && to < stack.0.len()
            {
                let item = stack.0.remove(from);
                stack.0.insert(to, item);
            }
        });
    }

    pub fn set_effect_enabled(&mut self, object_id: usize, index: usize, enabled: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_enabled(index, enabled);
            }
        });
        self.touch();
    }

    pub fn remove_effect(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.remove(index);
            }
        });
        self.touch();
    }

    pub fn set_effect_param(&mut self, object_id: usize, index: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_f32(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_bool(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: bool,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_bool(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_text(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: String,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_text(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_path(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: String,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_path(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_enum(&mut self, object_id: usize, index: usize, key: &str, value: u32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_enum(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_track_ref(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: i32,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_track_ref(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_keyframe(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_keyframe(index, key, frame, value, engine_id, engine_payload);
            }
        });
        self.touch();
    }

    pub fn remove_effect_keyframe(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        frame: i32,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.remove_keyframe(index, key, frame);
            }
        });
        self.touch();
    }

    pub fn get_effect_keyframes(
        &self,
        object_id: usize,
        index: usize,
        key: &str,
    ) -> Vec<crate::ecs::types::Keyframe> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks
                .get(entity)
                .ok()
                .and_then(|s| s.0.get(index))
                .and_then(|e| e.params.get(key))
                .map(|p| p.keyframes.clone())
                .unwrap_or_default()
        })
    }

    pub fn effect_stack_of(&self, object_id: usize) -> Vec<crate::ecs::types::EffectInstance> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks.get(entity).map(|s| s.0.clone()).unwrap_or_default()
        })
    }

    pub fn effect_param_f32(&self, object_id: usize, index: usize, key: &str) -> Option<f32> {
        let entity = self.find_entity(object_id)?;
        self.world.run(|stacks: View<EffectStack>| {
            match stacks
                .get(entity)
                .ok()?
                .0
                .get(index)?
                .params
                .get(key)?
                .static_value
            {
                crate::ecs::types::Value::Number(v) => Some(v),
                _ => None,
            }
        })
    }

    pub fn effect_param_bool(&self, object_id: usize, index: usize, key: &str) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|stacks: View<EffectStack>| {
            matches!(
                stacks
                    .get(entity)
                    .ok()
                    .and_then(|s| s.0.get(index))
                    .and_then(|e| e.params.get(key))
                    .map(|p| &p.static_value),
                Some(crate::ecs::types::Value::Bool(true))
            )
        })
    }

    #[allow(dead_code)]
    pub fn save_effect_preset(
        &mut self,
        object_id: usize,
        effect_index: usize,
        preset_name: &str,
    ) -> Result<crate::ecs::resources::PresetData, String> {
        let Some(entity) = self.find_entity(object_id) else {
            return Err("Object not found".to_string());
        };

        let (effect_id, params) = self.world.run(|stacks: View<EffectStack>| {
            let stack = stacks
                .get(entity)
                .map_err(|_| "EffectStack not found".to_string())?;
            let instance = stack
                .0
                .get(effect_index)
                .ok_or_else(|| "Effect index out of range".to_string())?;
            let mut list = Vec::new();
            for (key, param) in &instance.params {
                if let crate::ecs::types::Value::Number(val) = param.static_value {
                    list.push((key.clone(), val));
                }
            }
            list.sort_by(|a, b| a.0.cmp(&b.0));
            Ok::<_, String>((instance.effect_id.clone(), list))
        })?;

        let effect_uuid = crate::effects::loader::by_id(&effect_id)
            .map(|s| s.uuid().to_string())
            .unwrap_or(effect_id);

        let mut store = self
            .world
            .borrow::<UniqueViewMut<crate::ecs::resources::PresetStore>>()
            .map_err(|e| e.to_string())?;
        store
            .save_preset(&effect_uuid, preset_name, params)
            .map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn apply_effect_preset(
        &mut self,
        object_id: usize,
        effect_index: usize,
        preset_id: &str,
    ) -> Result<bool, String> {
        let Some(entity) = self.find_entity(object_id) else {
            return Err("Object not found".to_string());
        };

        let effect_id = self.world.run(|stacks: View<EffectStack>| {
            let stack = stacks
                .get(entity)
                .map_err(|_| "EffectStack not found".to_string())?;
            let instance = stack
                .0
                .get(effect_index)
                .ok_or_else(|| "Effect index out of range".to_string())?;
            Ok::<_, String>(instance.effect_id.clone())
        })?;

        let effect_uuid = crate::effects::loader::by_id(&effect_id)
            .map(|s| s.uuid().to_string())
            .unwrap_or(effect_id);

        let preset = {
            let store = self
                .world
                .borrow::<UniqueView<crate::ecs::resources::PresetStore>>()
                .map_err(|e| e.to_string())?;
            store.find_preset(&effect_uuid, preset_id).cloned()
        };

        let Some(preset) = preset else {
            return Ok(false);
        };

        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                if let Some(instance) = stack.0.get_mut(effect_index) {
                    for (key, val) in preset.params {
                        if let Some(param) = instance.params.get_mut(&key) {
                            param.static_value = crate::ecs::types::Value::Number(val);
                        }
                    }
                }
            }
        });

        self.touch();
        Ok(true)
    }

    #[allow(dead_code)]
    pub fn get_effect_presets_for(
        &self,
        effect_id_or_uuid: &str,
    ) -> Vec<crate::ecs::resources::PresetData> {
        let effect_uuid = crate::effects::loader::by_id(effect_id_or_uuid)
            .map(|s| s.uuid().to_string())
            .unwrap_or_else(|| effect_id_or_uuid.to_string());

        self.world
            .borrow::<UniqueView<crate::ecs::resources::PresetStore>>()
            .map(|store| store.get_presets_for_effect(&effect_uuid).to_vec())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn delete_effect_preset(
        &mut self,
        effect_id_or_uuid: &str,
        preset_id: &str,
    ) -> Result<bool, String> {
        let effect_uuid = crate::effects::loader::by_id(effect_id_or_uuid)
            .map(|s| s.uuid().to_string())
            .unwrap_or_else(|| effect_id_or_uuid.to_string());

        let mut store = self
            .world
            .borrow::<UniqueViewMut<crate::ecs::resources::PresetStore>>()
            .map_err(|e| e.to_string())?;
        store
            .delete_preset(&effect_uuid, preset_id)
            .map_err(|e| e.to_string())
    }
}
