use super::curtain::{ControllerKind, CurtainInfo};
use crate::ecs::transform::{Camera, GlobalMatrix, Projection, TargetLayerMode, view_space_depth};
use std::collections::HashMap;

pub(crate) fn projection_for(_kind_id: u32, fov_deg: f32) -> Projection {
    Projection::Perspective { fov_deg }
}

pub(crate) fn resolve_camera(
    chain_idx: &[usize],
    controllers: &[CurtainInfo],
    layer_positions: &HashMap<i32, (f32, f32, f32)>,
) -> Option<(i32, Camera)> {
    for &i in chain_idx {
        if let ControllerKind::Group {
            camera: Some(cam), ..
        } = controllers[i].kind
        {
            let mut cam = cam;
            if let TargetLayerMode::Layer(n) = cam.target_layer_mode {
                if let Some(&(lx, ly, lz)) = layer_positions.get(&n) {
                    cam.target_x += lx;
                    cam.target_y += ly;
                    cam.target_z += lz;
                }
            }
            return Some((controllers[i].layer, cam));
        }
    }
    None
}

pub(crate) fn zbuffer_sort_key(camera_layer: i32, global: &GlobalMatrix, cam: &Camera) -> f32 {
    let depth = view_space_depth(global, cam);
    let span = (cam.far - cam.near).max(1e-3);
    let normalized = ((depth - cam.near) / span - 0.5).clamp(-0.5, 0.5);
    camera_layer as f32 + normalized
}
