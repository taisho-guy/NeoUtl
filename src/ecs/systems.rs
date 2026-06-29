// src/ecs/systems.rs
use super::EcsWorld;
use crate::ecs::components::{KindId, TextContent, TimeRange};
use crate::ecs::resources::TimelineResource;
use shipyard::{Get, IntoIter, UniqueView, View};

pub struct ActiveObject {
    pub kind_id: u32,
    pub text_content: Option<TextContent>,
}

pub fn get_active_objects_system(world: &EcsWorld) -> Vec<ActiveObject> {
    world.world.run(
        |timeline: UniqueView<TimelineResource>,
         time_ranges: View<TimeRange>,
         kind_ids: View<KindId>,
         text_contents: View<TextContent>| {
            let current = timeline.current_frame;
            let mut active = Vec::new();

            for (id, (range, kind)) in (&time_ranges, &kind_ids).iter().with_id() {
                if current >= range.start_frame && current < range.end_frame {
                    let text_content = text_contents.get(id).ok().cloned();
                    active.push(ActiveObject {
                        kind_id: kind.0,
                        text_content,
                    });
                }
            }
            active
        },
    )
}
