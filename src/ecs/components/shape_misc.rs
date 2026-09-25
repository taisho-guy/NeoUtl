use super::param_access::ParamAccess;
use crate::ecs::track::Track;
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
            if let Some(slot) = fill_color.get_mut(idx) {
                *slot = *v;
            }
        }
        let mut stroke_color = [0.0; 4];
        for (idx, v) in value.stroke_color.iter().take(4).enumerate() {
            if let Some(slot) = stroke_color.get_mut(idx) {
                *slot = *v;
            }
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
            "sides" => self.sides.to_string().parse().unwrap_or(f32::MAX),
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
            "sides" => {
                self.sides = value
                    .max(3.0)
                    .round()
                    .to_string()
                    .parse()
                    .unwrap_or(u32::MAX);
            }
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
        if let Some(existing) = track.iter_mut().find(|k| k.frame == frame) {
            existing.value = value;
            existing.engine_id = engine_id;
            existing.engine_payload = engine_payload;
            existing.edit_seq = crate::ecs::types::next_edit_seq();
        } else {
            track.push(crate::ecs::types::Keyframe::new(
                frame,
                value,
                engine_id,
                engine_payload,
            ));
            track.sort_by_key(|k| k.frame);
        }
    }

    pub fn remove_keyframe(&mut self, key: &str, frame: i32) {
        if let Some(track) = self.0.get_mut(key)
            && let Some(index) = track.index_of(frame)
        {
            let _ = track.remove_point(index);
        }
    }

    pub fn move_keyframe(&mut self, key: &str, old_frame: i32, new_frame: i32) -> bool {
        let Some(track) = self.0.get_mut(key) else {
            return false;
        };
        let Some(index) = track.index_of(old_frame) else {
            return false;
        };
        track.move_point(index, new_frame) == Ok(new_frame)
    }

    pub fn seed_start(&mut self, key: &str, start: i32, value: f32) {
        self.0
            .entry(key.to_owned())
            .or_default()
            .seed_start(start, value);
    }

    pub fn bind_range(&mut self, start: i32, end: i32) {
        for track in self.0.values_mut() {
            track.bind_range(start, end);
        }
    }

    pub fn shift(&mut self, delta: i32) {
        for track in self.0.values_mut() {
            track.shift(delta);
        }
    }

    pub fn split_at(
        &mut self,
        split_frame: i32,
        fallback_for: impl Fn(&str) -> Option<f32>,
    ) -> (KeyframeTracks, HashMap<String, f32>) {
        let mut second = HashMap::new();
        let mut evaluated = HashMap::new();
        for (key, track) in &mut self.0 {
            let fallback = fallback_for(key).unwrap_or(0.0);
            evaluated.insert(key.clone(), track.evaluate(split_frame, fallback));
            let tail = track.split_at_frame(split_frame, fallback);
            if !tail.is_empty() {
                second.insert(key.clone(), tail);
            }
        }
        (KeyframeTracks(second), evaluated)
    }

    pub fn apply(&self, target: &mut impl ParamAccess, frame: i32) {
        for (key, track) in &self.0 {
            let Some(fallback) = target.get_param(key) else {
                continue;
            };
            target.set_param(key, track.evaluate(frame, fallback));
        }
    }
}

#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct MediaSource {
    pub path: std::path::PathBuf,
    pub kind: neoutl_media_runtime::MediaKind,
    pub trim_in_frame: i64,
}
