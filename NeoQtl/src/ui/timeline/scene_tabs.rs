use crate::app_state::{self, SharedAppState};
use crate::ui::types::SceneTabItem;

pub fn list_scenes(state: &SharedAppState) -> Vec<SceneTabItem> {
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    world
        .scenes()
        .into_iter()
        .map(|s| SceneTabItem {
            id: s.id,
            name: s.name,
            active: s.id == active,
        })
        .collect()
}

pub fn switch_scene(state: &SharedAppState, scene_id: i32) -> bool {
    let world_holder = app_state::active_world(state);
    let mut world = world_holder.lock().unwrap();
    world.switch_scene(scene_id)
}

pub fn add_scene(state: &SharedAppState, name: &str) -> i32 {
    let world_holder = app_state::active_world(state);
    app_state::snapshot_before_edit(state);
    let mut world = world_holder.lock().unwrap();
    world.add_scene(name)
}

pub fn remove_scene(state: &SharedAppState, scene_id: i32) -> bool {
    let world_holder = app_state::active_world(state);
    app_state::snapshot_before_edit(state);
    let mut world = world_holder.lock().unwrap();
    world.remove_scene(scene_id)
}

pub fn reorder_scene(_state: &SharedAppState, _from: i32, _to: i32) -> bool {
    false
}
