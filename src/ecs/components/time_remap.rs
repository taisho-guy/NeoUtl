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
        f32_to_i32(self.keyframes.evaluate(layer_frame, i32_to_f32(fallback)))
    }

    pub fn set_key(&mut self, frame: i32, value: i32, engine_id: String) {
        if let Some(existing) = self.keyframes.iter_mut().find(|k| k.frame == frame) {
            existing.value = i32_to_f32(value);
            existing.engine_id = engine_id;
            existing.edit_seq = next_edit_seq();
        } else {
            self.keyframes.push(Keyframe::new(
                frame,
                i32_to_f32(value),
                engine_id,
                Vec::new(),
            ));
            self.keyframes.sort_by_key(|k| k.frame);
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

fn i32_to_f32(value: i32) -> f32 {
    value.to_string().parse().unwrap_or_else(|_| {
        if value.is_negative() {
            f32::MIN
        } else {
            f32::MAX
        }
    })
}

fn f32_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().to_string().parse().unwrap_or_else(|_| {
        if value.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}
