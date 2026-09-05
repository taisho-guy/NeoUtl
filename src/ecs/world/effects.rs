use crate::ecs::EcsWorld;
use crate::ecs::effects::{self, EffectStack};
use shipyard::{Get, View, ViewMut};

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
}
