use crate::ecs::EcsWorld;
use crate::ecs::components::{GroupControl, KeyframeTracks};
use shipyard::{Get, View, ViewMut};

impl EcsWorld {
    pub fn get_group_control(&self, object_id: usize) -> Option<GroupControl> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|controls: View<GroupControl>| controls.get(entity).ok().copied())
    }

    pub fn set_keyframe(
        &mut self,
        object_id: usize,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        let mut tracks = self
            .world
            .run(|t: View<KeyframeTracks>| t.get(entity).ok().cloned())
            .unwrap_or_default();
        tracks.set_keyframe(key, frame, value, engine_id, engine_payload);
        self.world.add_component(entity, tracks);
        self.touch();
    }

    pub fn remove_keyframe(&mut self, object_id: usize, key: &str, frame: i32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut tracks: ViewMut<KeyframeTracks>| {
            if let Ok(mut t) = (&mut tracks).get(entity) {
                t.remove_keyframe(key, frame);
            }
        });
        self.touch();
    }

    pub fn move_keyframe(
        &mut self,
        object_id: usize,
        key: &str,
        old_frame: i32,
        new_frame: i32,
    ) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|mut tracks: ViewMut<KeyframeTracks>| {
            (&mut tracks)
                .get(entity)
                .ok()
                .map(|mut t| t.move_keyframe(key, old_frame, new_frame))
                .unwrap_or(false)
        })
    }

    pub fn get_keyframes(&self, object_id: usize, key: &str) -> Vec<crate::ecs::types::Keyframe> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|t: View<KeyframeTracks>| {
            t.get(entity)
                .ok()
                .and_then(|tracks| tracks.0.get(key).cloned())
                .unwrap_or_default()
        })
    }
}
