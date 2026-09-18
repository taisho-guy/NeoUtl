use super::{LAYER_HEIGHT, TimelineWindow};
use crate::app_state::{self, SharedAppState};

impl TimelineWindow {
    pub fn frame_to_x(&self, frame: i32) -> f32 {
        frame as f32 * self.zoom_scale - self.scroll_x
    }

    pub fn px_to_frame(&self, px: f32) -> i32 {
        ((px + self.scroll_x) / self.zoom_scale).floor().max(0.0) as i32
    }

    pub fn layer_to_y(&self, layer: i32) -> f32 {
        layer as f32 * LAYER_HEIGHT - self.scroll_y
    }

    pub fn px_to_layer(&self, py: f32) -> i32 {
        ((py + self.scroll_y) / LAYER_HEIGHT).floor().max(0.0) as i32
    }

    pub fn visible_frame_range(&self, viewport_width: f32) -> (i32, i32) {
        (self.px_to_frame(0.0), self.px_to_frame(viewport_width))
    }

    pub fn content_width(&self, total_frames: i32) -> f32 {
        total_frames.max(0) as f32 * self.zoom_scale
    }

    pub fn max_scroll_x(&self, total_frames: i32, viewport_width: f32) -> f32 {
        (self.content_width(total_frames) - viewport_width).max(0.0)
    }

    pub fn set_scroll_x(&mut self, value: f32, total_frames: i32, viewport_width: f32) {
        self.scroll_x = value.clamp(0.0, self.max_scroll_x(total_frames, viewport_width));
    }

    pub fn set_scroll_y(&mut self, value: f32) {
        self.scroll_y = value.max(0.0);
    }

    pub fn set_zoom_scale(&mut self, value: f32) {
        self.zoom_scale = value.clamp(0.05, 100.0);
    }

    pub fn auto_scroll_target(&self, current_frame: i32, viewport_width: f32) -> Option<f32> {
        let (first, last) = self.visible_frame_range(viewport_width);
        if current_frame >= first && current_frame <= last {
            return None;
        }
        Some((current_frame as f32 * self.zoom_scale - viewport_width * 0.5).max(0.0))
    }

    pub fn grid_interval(&self, state: &SharedAppState) -> i32 {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let active_scene = world.active_scene();
        world
            .scenes()
            .into_iter()
            .find(|scene| scene.id == active_scene)
            .map_or(30, |scene| scene.effective_grid_interval())
    }

    pub fn select_in_rect(&mut self, state: &SharedAppState, x0: f32, y0: f32, x1: f32, y1: f32) {
        let start_frame = self.px_to_frame(x0.min(x1));
        let end_frame = self.px_to_frame(x0.max(x1));
        let start_layer = self.px_to_layer(y0.min(y1));
        let end_layer = self.px_to_layer(y0.max(y1));
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        self.selected_ids = world
            .get_timeline_objects()
            .iter()
            .filter(|o| {
                o.start_frame < end_frame
                    && o.end_frame > start_frame
                    && o.layer >= start_layer
                    && o.layer <= end_layer
            })
            .map(|o| o.id)
            .collect();
        let ids = self.selected_ids.iter().map(|&i| i as usize).collect();
        drop(world);
        world_holder.lock().unwrap().set_selected_ids(ids);
    }
}
