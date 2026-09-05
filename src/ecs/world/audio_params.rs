use crate::ecs::EcsWorld;
use crate::ecs::components::{AudioParams, KindId};
use shipyard::{Get, View, ViewMut};

impl EcsWorld {
    pub fn set_audio_params(&mut self, object_id: usize, volume: f32, pan: f32, mute: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut audio: ViewMut<AudioParams>| {
            if let Ok(mut slot) = (&mut audio).get(entity) {
                slot.volume = volume;
                slot.pan = pan;
                slot.mute = mute;
            }
        });
    }

    pub fn get_audio_params(&self, object_id: usize) -> Option<AudioParams> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|audio: View<AudioParams>| audio.get(entity).ok().copied())
    }

    pub fn is_audio_object(&self, object_id: usize) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|kind_ids: View<KindId>| {
            kind_ids
                .get(entity)
                .ok()
                .and_then(|k| crate::objects::loader::by_kind_id(k.0))
                .is_some_and(|p| p.stable_id == neoutl_object_api::AUDIO_STABLE_ID)
        })
    }
}
