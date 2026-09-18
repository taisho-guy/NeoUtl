pub mod actions;
pub mod bridge;
pub mod clip_item;
pub mod context_menu;
pub mod data;
pub mod grid;
pub mod layer_header;
pub mod layer_menu;
pub mod ruler;
pub mod scene_tabs;
pub mod util;
pub mod view;

use crate::app_state::{self, SharedAppState};
use crate::ui::types::TimelineObject;
use std::collections::HashSet;

pub const HEADER_WIDTH: f32 = 60.0;
pub const LAYER_HEIGHT: f32 = 30.0;
pub const RULER_HEIGHT: f32 = 32.0;
pub const HANDLE_WIDTH: f32 = 10.0;
pub const KEYFRAME_SIZE: f32 = 8.0;

pub struct TimelineWindow {
    pub open: bool,
    pub zoom_scale: f32,
    pub selected_layer: i32,
    pub ripple_mode: bool,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub selected_ids: HashSet<i32>,
    pub select_range: Option<(i32, i32)>,
    pub show_grid: bool,
    pub show_waveform: bool,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl TimelineWindow {
    pub fn new() -> Self {
        Self {
            open: true,
            zoom_scale: 1.0,
            selected_layer: 0,
            ripple_mode: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            selected_ids: HashSet::new(),
            select_range: None,
            show_grid: true,
            show_waveform: true,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }

    pub fn get_timeline_objects(&self, state: &SharedAppState) -> Vec<TimelineObject> {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        let fps = proj.fps as f64;
        let objects = world.get_timeline_objects();
        objects
            .iter()
            .map(|d| {
                let mut obj = data::to_timeline_object(d, fps);
                obj.selected = self.selected_ids.contains(&obj.id);
                obj
            })
            .collect()
    }

    pub fn select_clip(&mut self, state: &SharedAppState, id: i32, multi: bool) {
        if !multi {
            self.selected_ids.clear();
        }
        self.selected_ids.insert(id);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        world.set_selected_ids(self.selected_ids.iter().map(|&i| i as usize).collect());
    }

    pub fn clear_selection(&mut self, state: &SharedAppState) {
        self.selected_ids.clear();
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        world.set_selected_ids(HashSet::new());
    }

    pub fn apply_clip_batch_move(
        &mut self,
        state: &SharedAppState,
        clip_id: i32,
        delta_layer: i32,
        delta_start: i32,
    ) {
        if delta_layer == 0 && delta_start == 0 {
            return;
        }
        let targets: Vec<i32> = if self.selected_ids.contains(&clip_id) {
            self.selected_ids.iter().copied().collect()
        } else {
            vec![clip_id]
        };
        let objects = self.get_timeline_objects(state);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let layer_max = world.layer_states().len() as i32 - 1;
        for id in targets {
            let Some(obj) = objects.iter().find(|o| o.id == id) else {
                continue;
            };
            let new_layer = (obj.layer + delta_layer).clamp(0, layer_max.max(0));
            let new_start = (obj.start_frame + delta_start).max(0);
            world.move_object(id as usize, new_start, new_layer);
        }
    }

    pub fn apply_clip_resize(
        &mut self,
        state: &SharedAppState,
        clip_id: i32,
        delta_start: i32,
        delta_duration: i32,
    ) {
        if delta_start == 0 && delta_duration == 0 {
            return;
        }
        let targets: Vec<i32> = if self.selected_ids.contains(&clip_id) {
            self.selected_ids.iter().copied().collect()
        } else {
            vec![clip_id]
        };
        let objects = self.get_timeline_objects(state);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        for id in targets {
            let Some(obj) = objects.iter().find(|o| o.id == id) else {
                continue;
            };
            let new_start = (obj.start_frame + delta_start).max(0);
            let duration = (obj.end_frame - obj.start_frame + delta_duration).max(1);
            world.resize_object(id as usize, new_start, new_start + duration);
        }
    }

    pub fn split_clip_at(&mut self, state: &SharedAppState, id: i32, frame: i32) {
        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        let _ = world.split_object(id as usize, frame);
    }

    pub fn delete_selected(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        for id in self.selected_ids.drain() {
            world.delete_object(id as usize);
        }
    }

    pub fn move_clip(&mut self, state: &SharedAppState, id: i32, start_frame: i32, layer: i32) {
        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        world.move_object(id as usize, start_frame, layer);
    }
}
