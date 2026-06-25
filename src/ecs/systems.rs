use super::EcsWorld;

pub fn check_active_objects_system(world: &EcsWorld) -> bool {
    let current = world.resources.current_frame;
    world
        .time_ranges
        .iter()
        .any(|t| current >= t.start_frame && current < t.end_frame)
}
