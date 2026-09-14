use super::param_access::ParamAccess;
use crate::ecs::transform::{Camera, TargetLayerMode};
use serde::{Deserialize, Serialize};
use shipyard::Component;

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

pub(super) fn bool_to_f32(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}
