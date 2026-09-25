use super::curtain::{ControllerKind, CurtainInfo};
use crate::ecs::transform::{Camera, GlobalMatrix, Projection, TargetLayerMode, view_space_depth};
use std::collections::HashMap;

pub(crate) fn projection_for(_kind_id: u32, fov_deg: f32) -> Projection {
    Projection::Perspective { fov_deg }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveCameraCandidate {
    pub layer: i32,
    pub camera: Camera,
}

pub(crate) fn resolve_camera(
    active_cameras: &[ActiveCameraCandidate],
    chain_idx: &[usize],
    controllers: &[CurtainInfo],
    layer_positions: &HashMap<i32, (f32, f32, f32)>,
) -> Option<(i32, Camera)> {
    if let Some(candidate) = active_cameras.first() {
        let mut cam = candidate.camera;
        if let TargetLayerMode::Layer(n) = cam.target_layer_mode
            && let Some(&(lx, ly, lz)) = layer_positions.get(&n)
        {
            cam.target_x += lx;
            cam.target_y += ly;
            cam.target_z += lz;
        }
        return Some((candidate.layer, cam));
    }

    for &i in chain_idx {
        let Some(controller) = controllers.get(i) else {
            continue;
        };
        if let ControllerKind::Group {
            camera: Some(cam), ..
        } = controller.kind
        {
            let mut cam = cam;
            if let TargetLayerMode::Layer(n) = cam.target_layer_mode
                && let Some(&(lx, ly, lz)) = layer_positions.get(&n)
            {
                cam.target_x += lx;
                cam.target_y += ly;
                cam.target_z += lz;
            }
            return Some((controller.layer, cam));
        }
    }
    None
}

pub(crate) fn zbuffer_sort_key(camera_layer: i32, global: &GlobalMatrix, cam: &Camera) -> f32 {
    let depth = view_space_depth(global, cam);
    let span = (cam.far - cam.near).max(1e-3);
    let normalized = ((depth - cam.near) / span - 0.5).clamp(-0.5, 0.5);
    camera_layer.to_string().parse::<f32>().unwrap_or_else(|_| {
        if camera_layer.is_negative() {
            f32::MIN
        } else {
            f32::MAX
        }
    }) + normalized
}
