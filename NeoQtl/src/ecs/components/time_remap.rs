use crate::ecs::types::{ApplyMode, Keyframe, next_edit_seq};
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
        let engine_id = &self.keyframes[0].engine_id;
        let raw: Vec<(i32, f32, Vec<u8>)> = self
            .keyframes
            .iter()
            .map(|k| (k.frame, k.value, k.engine_payload.clone()))
            .collect();
        let value = crate::easings::loader::by_id(engine_id)
            .map(|engine| engine.evaluate(&raw, layer_frame, fallback as f32))
            .unwrap_or(fallback as f32);
        value.round() as i32
    }

    pub fn set_key(&mut self, frame: i32, value: i32, engine_id: String) {
        let edit_seq = next_edit_seq();
        match self.keyframes.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value as f32;
                existing.engine_id = engine_id;
                existing.edit_seq = edit_seq;
            }
            None => {
                self.keyframes.push(Keyframe {
                    frame,
                    value: value as f32,
                    engine_id,
                    engine_payload: Vec::new(),
                    edit_seq,
                    apply_mode: ApplyMode::default(),
                });
                self.keyframes.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn remove_key(&mut self, frame: i32) {
        self.keyframes.retain(|k| k.frame != frame);
    }
}
