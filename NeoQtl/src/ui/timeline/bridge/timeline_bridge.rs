use crate::app_state::{self, SharedAppState};
use crate::ui::{self, UiState};
use std::sync::MutexGuard;

#[cxx::bridge(namespace = "neoqtl")]
mod ffi {
    #[derive(Clone, Debug, Default)]
    struct ClipData {
        id: i32,
        start_frame: i32,
        duration_frames: i32,
        layer: i32,
        color_index: i32,
        kind_known: bool,
        selected: bool,
        locked: bool,
        label: String,
        keyframe_frames: Vec<i32>,
    }

    #[derive(Clone, Debug, Default)]
    struct SceneTabData {
        id: i32,
        name: String,
        active: bool,
    }

    #[derive(Clone, Debug, Default)]
    struct GridSettingsData {
        mode: String,
        bpm: f32,
        offset: f32,
        interval: i32,
        subdivision: i32,
    }

    extern "Rust" {
        fn timeline_clips() -> Vec<ClipData>;
        fn timeline_scene_tabs() -> Vec<SceneTabData>;
        fn timeline_grid_settings() -> GridSettingsData;

        fn timeline_current_frame() -> i32;
        fn timeline_total_frames() -> i32;
        fn timeline_layer_count() -> i32;
        fn timeline_scale() -> f32;
        fn timeline_project_fps() -> i32;
        fn timeline_enable_snap() -> bool;
        fn timeline_magnetic_snap_range() -> i32;

        fn timeline_set_viewport(content_x: f32, content_y: f32, width: f32, height: f32);
        fn timeline_set_zoom(value: f32);
        fn timeline_seek(frame: i32);

        fn timeline_select_clip(id: i32, additive: bool);
        fn timeline_clear_selection();
        fn timeline_select_in_rect(x0: f32, y0: f32, x1: f32, y1: f32);

        fn timeline_apply_clip_batch_move(clip_id: i32, delta_layer: i32, delta_start: i32);
        fn timeline_apply_clip_resize(clip_id: i32, delta_start: i32, delta_duration: i32);
        fn timeline_split_clip(id: i32, frame: i32);
        fn timeline_delete_selected();
        fn timeline_move_keyframe(id: i32, from_frame: i32, to_frame: i32);

        fn timeline_switch_scene(scene_id: i32) -> bool;
        fn timeline_add_scene(name: &str) -> i32;
        fn timeline_remove_scene(scene_id: i32) -> bool;
    }
}

use ffi::{ClipData, GridSettingsData, SceneTabData};

fn ui() -> MutexGuard<'static, UiState> {
    ui::ui_state().lock().unwrap()
}

macro_rules! with_state {
    ($guard:ident, $state:ident, $default:expr) => {
        let $guard = ui();
        let Some($state) = $guard.app_state.as_ref() else {
            return $default;
        };
    };
}

macro_rules! with_timeline_mut {
    ($guard:ident, $state:ident, $timeline:ident) => {
        let mut $guard = ui();
        let Some($state) = $guard.app_state.clone() else {
            return;
        };
        let Some($timeline) = $guard.timeline.as_mut() else {
            return;
        };
    };
}

pub fn timeline_clips() -> Vec<ClipData> {
    let guard = ui();
    let (Some(state), Some(timeline)) = (guard.app_state.as_ref(), guard.timeline.as_ref()) else {
        return Vec::new();
    };
    let layer_states = {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        world.layer_states()
    };
    timeline
        .get_timeline_objects(state)
        .iter()
        .map(|o| ClipData {
            id: o.id,
            start_frame: o.start_frame,
            duration_frames: (o.end_frame - o.start_frame).max(1),
            layer: o.layer,
            color_index: o.kind.max(0),
            kind_known: o.kind_known,
            selected: o.selected,
            locked: layer_states.get(o.layer as usize).is_some_and(|s| s.1),
            label: o.label.clone(),
            keyframe_frames: o.keyframe_frames.clone(),
        })
        .collect()
}

pub fn timeline_scene_tabs() -> Vec<SceneTabData> {
    with_state!(guard, state, Vec::new());
    crate::ui::timeline::scene_tabs::list_scenes(state)
        .into_iter()
        .map(|s| SceneTabData {
            id: s.id,
            name: s.name,
            active: s.active,
        })
        .collect()
}

pub fn timeline_grid_settings() -> GridSettingsData {
    with_state!(guard, state, GridSettingsData::default());
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    let Some(scene) = world.scenes().into_iter().find(|s| s.id == active) else {
        return GridSettingsData::default();
    };
    GridSettingsData {
        mode: match scene.grid_mode {
            1 => "BPM".to_string(),
            2 => "Frame".to_string(),
            _ => "Auto".to_string(),
        },
        bpm: scene.grid_bpm,
        offset: scene.grid_offset,
        interval: scene.grid_interval,
        subdivision: scene.grid_subdivision,
    }
}

pub fn timeline_current_frame() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let frame = world_holder.lock().unwrap().current_frame();
    frame
}

pub fn timeline_total_frames() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let frames = world_holder.lock().unwrap().total_frames();
    frames
}

pub fn timeline_layer_count() -> i32 {
    with_state!(guard, state, 0);
    let world_holder = app_state::active_world(state);
    let count = world_holder.lock().unwrap().layer_states().len() as i32;
    count
}

pub fn timeline_scale() -> f32 {
    let guard = ui();
    guard.timeline.as_ref().map_or(1.0, |t| t.zoom_scale)
}

pub fn timeline_project_fps() -> i32 {
    with_state!(guard, state, 30);
    let world_holder = app_state::active_world(state);
    let fps = world_holder.lock().unwrap().get_project().fps as i32;
    fps
}

pub fn timeline_enable_snap() -> bool {
    with_state!(guard, state, true);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    world
        .scenes()
        .into_iter()
        .find(|s| s.id == active)
        .is_none_or(|s| s.enable_snap)
}

pub fn timeline_magnetic_snap_range() -> i32 {
    with_state!(guard, state, 5);
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let active = world.active_scene();
    world
        .scenes()
        .into_iter()
        .find(|s| s.id == active)
        .map_or(5, |s| s.magnetic_snap_range)
}

pub fn timeline_set_viewport(content_x: f32, content_y: f32, width: f32, height: f32) {
    let mut guard = ui();
    let Some(timeline) = guard.timeline.as_mut() else {
        return;
    };
    timeline.scroll_x = content_x.max(0.0);
    timeline.scroll_y = content_y.max(0.0);
    timeline.viewport_width = width;
    timeline.viewport_height = height;
}

pub fn timeline_set_zoom(value: f32) {
    let mut guard = ui();
    if let Some(timeline) = guard.timeline.as_mut() {
        timeline.zoom_scale = value.clamp(0.05, 100.0);
    }
}

pub fn timeline_seek(frame: i32) {
    with_state!(guard, state, ());
    let world_holder = app_state::active_world(state);
    world_holder.lock().unwrap().set_current_frame(frame.max(0));
}

pub fn timeline_select_clip(id: i32, additive: bool) {
    with_timeline_mut!(guard, state, timeline);
    timeline.select_clip(&state, id, additive);
}

pub fn timeline_clear_selection() {
    with_timeline_mut!(guard, state, timeline);
    timeline.clear_selection(&state);
}

pub fn timeline_select_in_rect(x0: f32, y0: f32, x1: f32, y1: f32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.select_in_rect(&state, x0, y0, x1, y1);
}

pub fn timeline_apply_clip_batch_move(clip_id: i32, delta_layer: i32, delta_start: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.apply_clip_batch_move(&state, clip_id, delta_layer, delta_start);
}

pub fn timeline_apply_clip_resize(clip_id: i32, delta_start: i32, delta_duration: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.apply_clip_resize(&state, clip_id, delta_start, delta_duration);
}

pub fn timeline_split_clip(id: i32, frame: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.split_clip_at(&state, id, frame);
}

pub fn timeline_delete_selected() {
    with_timeline_mut!(guard, state, timeline);
    timeline.delete_selected(&state);
}

pub fn timeline_move_keyframe(id: i32, from_frame: i32, to_frame: i32) {
    with_timeline_mut!(guard, state, timeline);
    timeline.keyframe_moved(&state, id, from_frame, to_frame);
}

pub fn timeline_switch_scene(scene_id: i32) -> bool {
    with_state!(guard, state, false);
    crate::ui::timeline::scene_tabs::switch_scene(state, scene_id)
}

pub fn timeline_add_scene(name: &str) -> i32 {
    with_state!(guard, state, -1);
    crate::ui::timeline::scene_tabs::add_scene(state, name)
}

pub fn timeline_remove_scene(scene_id: i32) -> bool {
    with_state!(guard, state, false);
    crate::ui::timeline::scene_tabs::remove_scene(state, scene_id)
}

fn _assert_shared(_: &SharedAppState) {}
