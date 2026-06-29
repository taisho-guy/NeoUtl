// src/ecs/systems.rs
use super::EcsWorld;
use crate::ecs::components::TextContent;

pub struct ActiveObject {
    pub kind_id: u32,
    pub text_content: Option<TextContent>,
}

pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    let current = world.resources.current_frame;

    world
        .time_ranges
        .iter()
        .zip(world.kind_ids.iter())
        .zip(world.text_contents.iter())
        .filter_map(|((range, &kind_id), text)| {
            (current >= range.start_frame && current < range.end_frame).then(|| ActiveObject {
                kind_id,
                text_content: text.clone(),
            })
        })
        .collect()
}
