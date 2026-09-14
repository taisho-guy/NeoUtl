use super::param_access::ParamAccess;
use serde::{Deserialize, Serialize};
use shipyard::Component;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct ShapeParams {
    pub sides: u32,
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub extrude_depth: f32,
}

impl From<&ShapeParams> for neoutl_schema::ShapeParams {
    fn from(value: &ShapeParams) -> Self {
        Self {
            sides: value.sides,
            fill_color: value.fill_color.to_vec(),
            stroke_color: value.stroke_color.to_vec(),
            stroke_width: value.stroke_width,
            extrude_depth: value.extrude_depth,
        }
    }
}

impl TryFrom<&neoutl_schema::ShapeParams> for ShapeParams {
    type Error = String;

    fn try_from(value: &neoutl_schema::ShapeParams) -> Result<Self, Self::Error> {
        let mut fill_color = [0.0; 4];
        for (idx, v) in value.fill_color.iter().take(4).enumerate() {
            fill_color[idx] = *v;
        }
        let mut stroke_color = [0.0; 4];
        for (idx, v) in value.stroke_color.iter().take(4).enumerate() {
            stroke_color[idx] = *v;
        }
        Ok(Self {
            sides: value.sides,
            fill_color,
            stroke_color,
            stroke_width: value.stroke_width,
            extrude_depth: value.extrude_depth,
        })
    }
}

impl Default for ShapeParams {
    fn default() -> Self {
        Self {
            sides: 4,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            stroke_color: [0.0, 0.0, 0.0, 0.0],
            stroke_width: 0.0,
            extrude_depth: 0.0,
        }
    }
}

impl ParamAccess for ShapeParams {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "sides" => self.sides as f32,
            "extrude_depth" => self.extrude_depth,
            "stroke_width" => self.stroke_width,
            "fill_r" => self.fill_color[0],
            "fill_g" => self.fill_color[1],
            "fill_b" => self.fill_color[2],
            "fill_a" => self.fill_color[3],
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "sides" => self.sides = value.max(3.0) as u32,
            "extrude_depth" => self.extrude_depth = value.max(0.0),
            "stroke_width" => self.stroke_width = value.max(0.0),
            "fill_r" => self.fill_color[0] = value,
            "fill_g" => self.fill_color[1] = value,
            "fill_b" => self.fill_color[2] = value,
            "fill_a" => self.fill_color[3] = value,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct PluginParams(pub HashMap<String, f32>);

#[derive(Clone, Debug, Default, Component, Serialize, Deserialize)]
pub struct KeyframeTracks(pub HashMap<String, Vec<crate::ecs::types::Keyframe>>);

impl KeyframeTracks {
    pub fn set_keyframe(
        &mut self,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let track = self.0.entry(key.to_owned()).or_default();
        let edit_seq = crate::ecs::types::next_edit_seq();
        match track.iter_mut().find(|k| k.frame == frame) {
            Some(existing) => {
                existing.value = value;
                existing.engine_id = engine_id;
                existing.engine_payload = engine_payload;
                existing.edit_seq = edit_seq;
            }
            None => {
                track.push(crate::ecs::types::Keyframe {
                    frame,
                    value,
                    engine_id,
                    engine_payload,
                    edit_seq,
                    apply_mode: crate::ecs::types::ApplyMode::default(),
                });
                track.sort_by_key(|k| k.frame);
            }
        }
    }

    pub fn remove_keyframe(&mut self, key: &str, frame: i32) {
        if let Some(track) = self.0.get_mut(key) {
            track.retain(|k| k.frame != frame);
            if track.is_empty() {
                self.0.remove(key);
            }
        }
    }

    pub fn move_keyframe(&mut self, key: &str, old_frame: i32, new_frame: i32) -> bool {
        let Some(track) = self.0.get_mut(key) else {
            return false;
        };
        if old_frame == new_frame {
            return true;
        }
        if track.iter().any(|k| k.frame == new_frame) {
            return false;
        }
        let Some(k) = track.iter_mut().find(|k| k.frame == old_frame) else {
            return false;
        };
        k.frame = new_frame;
        track.sort_by_key(|k| k.frame);
        true
    }

    pub fn clamp_to_range(
        &mut self,
        _old_start: i32,
        _old_end: i32,
        _new_start: i32,
        _new_end: i32,
    ) {
    }

    pub fn shift(&mut self, delta: i32) {
        for track in self.0.values_mut() {
            for k in track.iter_mut() {
                k.frame += delta;
            }
        }
    }

    pub fn split_at(
        &mut self,
        split_frame: i32,
        fallback_for: impl Fn(&str) -> Option<f32>,
    ) -> (KeyframeTracks, HashMap<String, f32>) {
        let mut second = HashMap::new();
        let mut evaluated = HashMap::new();

        for (key, track) in self.0.iter_mut() {
            let fallback = fallback_for(key).unwrap_or(0.0);
            let eval_val = if track.is_empty() {
                fallback
            } else {
                let first_engine = &track[0].engine_id;
                let eng = crate::easings::loader::by_id(first_engine);
                let raw: Vec<(i32, f32, Vec<u8>)> = track
                    .iter()
                    .map(|k| (k.frame, k.value, k.engine_payload.clone()))
                    .collect();
                if let Some(e) = eng {
                    e.evaluate(&raw, split_frame, fallback)
                } else {
                    fallback
                }
            };
            evaluated.insert(key.clone(), eval_val);

            let second_track: Vec<_> = track
                .iter()
                .filter(|k| k.frame > split_frame)
                .cloned()
                .collect();
            track.retain(|k| k.frame < split_frame);
            if !second_track.is_empty() {
                second.insert(key.clone(), second_track);
            }
        }
        self.0.retain(|_, track| !track.is_empty());

        (KeyframeTracks(second), evaluated)
    }

    pub fn apply(&self, target: &mut impl ParamAccess, frame: i32) {
        for (key, track) in &self.0 {
            let Some(fallback) = target.get_param(key) else {
                continue;
            };
            let val = if track.is_empty() {
                fallback
            } else {
                let first_engine = &track[0].engine_id;
                let eng = crate::easings::loader::by_id(first_engine);
                let raw: Vec<(i32, f32, Vec<u8>)> = track
                    .iter()
                    .map(|k| (k.frame, k.value, k.engine_payload.clone()))
                    .collect();
                if let Some(e) = eng {
                    e.evaluate(&raw, frame, fallback)
                } else {
                    fallback
                }
            };
            target.set_param(key, val);
        }
    }
}

#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct MediaSource {
    pub path: std::path::PathBuf,
    pub kind: neoutl_media_runtime::MediaKind,
    pub trim_in_frame: i64,
}
