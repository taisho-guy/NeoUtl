use crate::ecs::track::Track;
use crate::ecs::types::{Keyframe, next_edit_seq};
use shipyard::Component;

#[derive(Clone, Debug, Default, Component)]
pub struct TimeRemap {
    pub keyframes: Vec<Keyframe>,
    pub freeze_frame: Option<i32>,
}

impl TimeRemap {
    pub fn resolve(&self, layer_frame: i32, fallback: i32) -> i32 {
        if let Some(frame) = self.freeze_frame {
            return frame;
        }
        if self.keyframes.is_empty() {
            return fallback;
        }
        self.keyframes
            .evaluate(layer_frame, fallback as f32)
            .round() as i32
    }

    pub fn set_key(&mut self, frame: i32, value: i32, engine_id: String) {
        match self.keyframes.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value as f32;
                existing.engine_id = engine_id;
                existing.edit_seq = next_edit_seq();
            }
            None => {
                self.keyframes
                    .push(Keyframe::new(frame, value as f32, engine_id, Vec::new()));
                self.keyframes.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn move_key(&mut self, index: usize, frame: i32) -> Option<i32> {
        self.keyframes.move_key(index, frame).ok()
    }

    pub fn remove_key(&mut self, frame: i32) {
        if let Some(index) = self.keyframes.index_of(frame) {
            self.keyframes.remove(index);
        }
    }
}
