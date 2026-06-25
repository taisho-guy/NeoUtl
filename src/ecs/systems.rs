// src/ecs/systems.rs
use super::EcsWorld;
use crate::objects::RenderKind;

pub fn get_active_render_kinds_system(world: &EcsWorld) -> Vec<RenderKind> {
    let current = world.resources.current_frame;
    let mut active_kinds = Vec::new();

    for (range, &kind) in world.time_ranges.iter().zip(world.render_kinds.iter()) {
        if current >= range.start_frame && current < range.end_frame {
            active_kinds.push(kind);
        }
    }
    active_kinds
}
