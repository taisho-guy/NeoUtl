use crate::ecs::components::ClipMode;
use crate::ecs::transform::{Camera, GlobalMatrix};
use crate::ecs::types::Value;
use shipyard::EntityId;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(crate) enum ControllerKind {
    Group {
        generate_framebuffer: bool,
        hide_captured: bool,
        camera: Option<Camera>,
    },
    Clip {
        mode: ClipMode,
        chroma_hue: f32,
        chroma_tolerance: f32,
        blend_edge: bool,
    },
}

pub(crate) struct CurtainInfo {
    pub(crate) entity: EntityId,
    pub(crate) layer: i32,
    pub(crate) span: (u32, u32),
    pub(crate) matrix: GlobalMatrix,
    pub(crate) effects: Vec<(String, HashMap<String, Value>)>,
    pub(crate) opacity: f32,
    pub(crate) kind: ControllerKind,
    pub(crate) render_self: bool,
}

impl CurtainInfo {
    pub(crate) fn requires_fb(&self) -> bool {
        match self.kind {
            ControllerKind::Group {
                generate_framebuffer,
                ..
            } => generate_framebuffer,
            ControllerKind::Clip { .. } => true,
        }
    }

    pub(crate) fn hide_captured(&self) -> bool {
        match self.kind {
            ControllerKind::Group { hide_captured, .. } => hide_captured,
            ControllerKind::Clip { .. } => false,
        }
    }
}

pub(crate) fn curtain_covers_layer(
    curtain_layer: i32,
    span: (u32, u32),
    target_layer: i32,
) -> bool {
    let (down, up) = span;
    if target_layer > curtain_layer {
        target_layer <= curtain_layer + down as i32
    } else if target_layer < curtain_layer {
        target_layer >= curtain_layer - up as i32
    } else {
        false
    }
}

pub(crate) fn group_only(chain: &[usize], controllers: &[CurtainInfo]) -> Vec<usize> {
    chain
        .iter()
        .copied()
        .filter(|&i| matches!(controllers[i].kind, ControllerKind::Group { .. }))
        .collect()
}

pub(crate) fn resolve_group_chain(
    obj_layer: i32,
    controllers: &[CurtainInfo],
    max_depth: i32,
) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut cursor_layer = obj_layer;
    loop {
        if chain.len() as i32 >= max_depth.max(0) {
            break;
        }
        let mut nearest: Option<(usize, i32)> = None;
        for (idx, c) in controllers.iter().enumerate() {
            if chain.contains(&idx) {
                continue;
            }
            if !curtain_covers_layer(c.layer, c.span, cursor_layer) {
                continue;
            }
            let dist = (cursor_layer - c.layer).abs();
            if nearest.is_none_or(|(_, d)| dist < d) {
                nearest = Some((idx, dist));
            }
        }
        let Some((idx, _)) = nearest else {
            break;
        };
        chain.push(idx);
        cursor_layer = controllers[idx].layer;
    }
    chain
}
