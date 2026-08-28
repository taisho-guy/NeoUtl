use crate::ecs::transform::{Camera, TargetLayerMode};
use serde::{Deserialize, Serialize};
use shipyard::Component;
use std::collections::HashMap;

pub trait ParamAccess {
    fn get_param(&self, key: &str) -> Option<f32>;
    fn set_param(&mut self, key: &str, value: f32) -> bool;
}

#[derive(Clone, Copy, Debug, Component)]
pub struct TimeRange {
    pub start_frame: i32,
    pub end_frame: i32,
}

#[derive(Clone, Copy, Debug, Component)]
pub struct ObjectId(pub usize);

#[derive(Clone, Copy, Debug, Component)]
pub struct KindId(pub u32);

#[derive(Clone, Copy, Debug, Component)]
pub struct Layer(pub i32);

#[derive(Clone, Copy, Debug, Component)]
pub struct SceneId(pub i32);

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct SceneObject {
    pub target_scene: i32,
}

#[derive(Clone, Copy, Debug, Component)]
pub struct GroupControl {
    pub layer_count_down: u32,
    pub layer_count_up: u32,
    pub generate_framebuffer: bool,
    pub hide_captured: bool,
    pub camera: Option<Camera>,
}

fn target_layer_mode_to_i32(m: TargetLayerMode) -> (i32, i32) {
    match m {
        TargetLayerMode::Origin => (0, 0),
        TargetLayerMode::CameraRelative => (1, 0),
        TargetLayerMode::Layer(n) => (2, n),
    }
}

fn target_layer_mode_from_i32(mode: i32, layer: i32) -> TargetLayerMode {
    match mode {
        1 => TargetLayerMode::CameraRelative,
        2 => TargetLayerMode::Layer(layer),
        _ => TargetLayerMode::Origin,
    }
}

impl From<&GroupControl> for neoutl_schema::GroupControl {
    fn from(value: &GroupControl) -> Self {
        Self {
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            generate_framebuffer: value.generate_framebuffer,
            hide_captured: value.hide_captured,
            camera: value.camera.map(|c| {
                let (mode, layer) = target_layer_mode_to_i32(c.target_layer_mode);
                neoutl_schema::CameraParams {
                    enabled: true,
                    pos_x: c.pos_x,
                    pos_y: c.pos_y,
                    pos_z: c.pos_z,
                    target_x: c.target_x,
                    target_y: c.target_y,
                    target_z: c.target_z,
                    near: c.near,
                    far: c.far,
                    tilt_deg: c.tilt_deg,
                    fov_deg: c.fov_deg,
                    target_layer_mode: mode,
                    target_layer: layer,
                    zbuffer_enabled: c.zbuffer_enabled,
                    focus_distance: c.focus_distance,
                    depth_blur_strength: c.depth_blur_strength,
                }
            }),
        }
    }
}

impl TryFrom<&neoutl_schema::GroupControl> for GroupControl {
    type Error = String;

    fn try_from(value: &neoutl_schema::GroupControl) -> Result<Self, Self::Error> {
        Ok(Self {
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            generate_framebuffer: value.generate_framebuffer,
            hide_captured: value.hide_captured,
            camera: value.camera.as_ref().filter(|c| c.enabled).map(|c| Camera {
                pos_x: c.pos_x,
                pos_y: c.pos_y,
                pos_z: c.pos_z,
                target_x: c.target_x,
                target_y: c.target_y,
                target_z: c.target_z,
                near: c.near,
                far: c.far,
                tilt_deg: c.tilt_deg,
                fov_deg: c.fov_deg,
                target_layer_mode: target_layer_mode_from_i32(c.target_layer_mode, c.target_layer),
                zbuffer_enabled: c.zbuffer_enabled,
                focus_distance: c.focus_distance,
                depth_blur_strength: c.depth_blur_strength,
            }),
        })
    }
}

impl Default for GroupControl {
    fn default() -> Self {
        Self {
            layer_count_down: 1,
            layer_count_up: 0,
            generate_framebuffer: false,
            hide_captured: false,
            camera: None,
        }
    }
}

impl ParamAccess for GroupControl {
    fn get_param(&self, key: &str) -> Option<f32> {
        let cam = self
            .camera
            .unwrap_or_else(|| Camera::for_resolution(1920.0, 1080.0));
        Some(match key {
            "layer_count_down" => self.layer_count_down as f32,
            "layer_count_up" => self.layer_count_up as f32,
            "generate_framebuffer" => bool_to_f32(self.generate_framebuffer),
            "hide_captured" => bool_to_f32(self.hide_captured),
            "camera_enabled" => bool_to_f32(self.camera.is_some()),
            "camera_pos_x" => cam.pos_x,
            "camera_pos_y" => cam.pos_y,
            "camera_pos_z" => cam.pos_z,
            "camera_target_x" => cam.target_x,
            "camera_target_y" => cam.target_y,
            "camera_target_z" => cam.target_z,
            "camera_tilt_deg" => cam.tilt_deg,
            "camera_fov_deg" => cam.fov_deg,
            "camera_target_layer_mode" => target_layer_mode_to_i32(cam.target_layer_mode).0 as f32,
            "camera_target_layer" => target_layer_mode_to_i32(cam.target_layer_mode).1 as f32,
            "camera_zbuffer_enabled" => bool_to_f32(cam.zbuffer_enabled),
            "camera_focus_distance" => cam.focus_distance,
            "camera_depth_blur_strength" => cam.depth_blur_strength,
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        if key == "camera_enabled" {
            self.camera = if value > 0.5 {
                Some(
                    self.camera
                        .unwrap_or_else(|| Camera::for_resolution(1920.0, 1080.0)),
                )
            } else {
                None
            };
            return true;
        }
        match key {
            "layer_count_down" => {
                self.layer_count_down = value.max(0.0) as u32;
                return true;
            }
            "layer_count_up" => {
                self.layer_count_up = value.max(0.0) as u32;
                return true;
            }
            "generate_framebuffer" => {
                self.generate_framebuffer = value > 0.5;
                return true;
            }
            "hide_captured" => {
                self.hide_captured = value > 0.5;
                return true;
            }
            _ => {}
        }
        let Some(cam) = self.camera.as_mut() else {
            return false;
        };
        match key {
            "camera_pos_x" => cam.pos_x = value,
            "camera_pos_y" => cam.pos_y = value,
            "camera_pos_z" => cam.pos_z = value,
            "camera_target_x" => cam.target_x = value,
            "camera_target_y" => cam.target_y = value,
            "camera_target_z" => cam.target_z = value,
            "camera_tilt_deg" => cam.tilt_deg = value,
            "camera_fov_deg" => cam.fov_deg = value.clamp(1.0, 179.0),
            "camera_target_layer_mode" => {
                let (_, layer) = target_layer_mode_to_i32(cam.target_layer_mode);
                cam.target_layer_mode = target_layer_mode_from_i32(value as i32, layer);
            }
            "camera_target_layer" => {
                cam.target_layer_mode = target_layer_mode_from_i32(2, value as i32);
            }
            "camera_zbuffer_enabled" => cam.zbuffer_enabled = value > 0.5,
            "camera_focus_distance" => cam.focus_distance = value,
            "camera_depth_blur_strength" => cam.depth_blur_strength = value.max(0.0),
            _ => return false,
        }
        true
    }
}

fn bool_to_f32(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipMode {
    Alpha = 0,
    AlphaInvert = 1,
    Luminance = 2,
    LuminanceInvert = 3,
    Chroma = 4,
}

impl Default for ClipMode {
    fn default() -> Self {
        ClipMode::Alpha
    }
}

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct ClipTarget {
    pub enabled: bool,
    pub layer_count_down: u32,
    pub layer_count_up: u32,
    pub mode: ClipMode,
    pub chroma_hue: f32,
    pub chroma_tolerance: f32,
    pub blend_edge: bool,
    pub render_self: bool,
}

impl From<&ClipTarget> for neoutl_schema::ClipTarget {
    fn from(value: &ClipTarget) -> Self {
        Self {
            enabled: value.enabled,
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            mode: match value.mode {
                ClipMode::Alpha => neoutl_schema::ClipMode::Alpha as i32,
                ClipMode::AlphaInvert => neoutl_schema::ClipMode::AlphaInvert as i32,
                ClipMode::Luminance => neoutl_schema::ClipMode::Luminance as i32,
                ClipMode::LuminanceInvert => neoutl_schema::ClipMode::LuminanceInvert as i32,
                ClipMode::Chroma => neoutl_schema::ClipMode::Chroma as i32,
            },
            chroma_hue: value.chroma_hue,
            chroma_tolerance: value.chroma_tolerance,
            blend_edge: value.blend_edge,
            render_self: value.render_self,
        }
    }
}

impl TryFrom<&neoutl_schema::ClipTarget> for ClipTarget {
    type Error = String;

    fn try_from(value: &neoutl_schema::ClipTarget) -> Result<Self, Self::Error> {
        Ok(Self {
            enabled: value.enabled,
            layer_count_down: value.layer_count_down,
            layer_count_up: value.layer_count_up,
            mode: match value.mode() {
                neoutl_schema::ClipMode::Alpha => ClipMode::Alpha,
                neoutl_schema::ClipMode::AlphaInvert => ClipMode::AlphaInvert,
                neoutl_schema::ClipMode::Luminance => ClipMode::Luminance,
                neoutl_schema::ClipMode::LuminanceInvert => ClipMode::LuminanceInvert,
                neoutl_schema::ClipMode::Chroma => ClipMode::Chroma,
            },
            chroma_hue: value.chroma_hue,
            chroma_tolerance: value.chroma_tolerance,
            blend_edge: value.blend_edge,
            render_self: value.render_self,
        })
    }
}

impl Default for ClipTarget {
    fn default() -> Self {
        Self {
            enabled: false,
            layer_count_down: 1,
            layer_count_up: 0,
            mode: ClipMode::Alpha,
            chroma_hue: 120.0,
            chroma_tolerance: 30.0,
            blend_edge: true,
            render_self: true,
        }
    }
}

impl ParamAccess for ClipTarget {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "enabled" => {
                if self.enabled {
                    1.0
                } else {
                    0.0
                }
            }
            "layer_count_down" => self.layer_count_down as f32,
            "layer_count_up" => self.layer_count_up as f32,
            "mode" => self.mode as u8 as f32,
            "chroma_hue" => self.chroma_hue,
            "chroma_tolerance" => self.chroma_tolerance,
            "blend_edge" => {
                if self.blend_edge {
                    1.0
                } else {
                    0.0
                }
            }
            "render_self" => {
                if self.render_self {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "enabled" => self.enabled = value > 0.5,
            "layer_count_down" => self.layer_count_down = value.max(0.0) as u32,
            "layer_count_up" => self.layer_count_up = value.max(0.0) as u32,
            "mode" => {
                self.mode = match value.round() as u8 {
                    0 => ClipMode::Alpha,
                    1 => ClipMode::AlphaInvert,
                    2 => ClipMode::Luminance,
                    3 => ClipMode::LuminanceInvert,
                    _ => ClipMode::Chroma,
                }
            }
            "chroma_hue" => self.chroma_hue = value.rem_euclid(360.0),
            "chroma_tolerance" => self.chroma_tolerance = value.clamp(0.0, 180.0),
            "blend_edge" => self.blend_edge = value > 0.5,
            "render_self" => self.render_self = value > 0.5,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Component, Serialize, Deserialize)]
pub struct AudioParams {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
}

impl From<&AudioParams> for neoutl_schema::AudioParams {
    fn from(value: &AudioParams) -> Self {
        Self {
            volume: value.volume,
            pan: value.pan,
            mute: value.mute,
        }
    }
}

impl TryFrom<&neoutl_schema::AudioParams> for AudioParams {
    type Error = String;

    fn try_from(value: &neoutl_schema::AudioParams) -> Result<Self, Self::Error> {
        Ok(Self {
            volume: value.volume,
            pan: value.pan,
            mute: value.mute,
        })
    }
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            mute: false,
        }
    }
}

impl ParamAccess for AudioParams {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "volume" => self.volume,
            "pan" => self.pan,
            "mute" => {
                if self.mute {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "volume" => self.volume = value,
            "pan" => self.pan = value,
            "mute" => self.mute = value > 0.5,
            _ => return false,
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub line_height: f32,
    pub outline_width: f32,
    pub outline_color: [f32; 4],
}

impl From<&TextContent> for neoutl_schema::TextContent {
    fn from(value: &TextContent) -> Self {
        Self {
            text: value.text.clone(),
            font_size: value.font_size,
            color: value.color.to_vec(),
            font_family: value.font_family.clone(),
            bold: value.bold,
            italic: value.italic,
            align: match value.align {
                TextAlign::Left => neoutl_schema::TextAlign::Left as i32,
                TextAlign::Center => neoutl_schema::TextAlign::Center as i32,
                TextAlign::Right => neoutl_schema::TextAlign::Right as i32,
            },
            line_height: value.line_height,
            outline_width: value.outline_width,
            outline_color: value.outline_color.to_vec(),
        }
    }
}

impl TryFrom<&neoutl_schema::TextContent> for TextContent {
    type Error = String;

    fn try_from(value: &neoutl_schema::TextContent) -> Result<Self, Self::Error> {
        let mut color = [0.0; 4];
        for (idx, v) in value.color.iter().take(4).enumerate() {
            color[idx] = *v;
        }
        let mut outline_color = [0.0; 4];
        for (idx, v) in value.outline_color.iter().take(4).enumerate() {
            outline_color[idx] = *v;
        }
        Ok(Self {
            text: value.text.clone(),
            font_size: value.font_size,
            color,
            font_family: value.font_family.clone(),
            bold: value.bold,
            italic: value.italic,
            align: match value.align() {
                neoutl_schema::TextAlign::Left => TextAlign::Left,
                neoutl_schema::TextAlign::Center => TextAlign::Center,
                neoutl_schema::TextAlign::Right => TextAlign::Right,
            },
            line_height: value.line_height,
            outline_width: value.outline_width,
            outline_color,
        })
    }
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_owned(),
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            font_family: String::new(),
            bold: false,
            italic: false,
            align: TextAlign::Left,
            line_height: 1.2,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl ParamAccess for TextContent {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "font_size" => self.font_size,
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "font_size" => self.font_size = value,
            _ => return false,
        }
        true
    }
}

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
    pub kind: crate::media::MediaKind,
    pub trim_in_frame: i64,
}
