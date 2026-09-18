use super::{HANDLE_WIDTH, LAYER_HEIGHT, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::ui::types::TimelineObject;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeHit {
    None,
    Left,
    Body,
    Right,
}

#[derive(Clone, Debug, Default)]
pub struct ClipViewModel {
    pub id: i32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color_index: i32,
    pub kind_known: bool,
    pub selected: bool,
    pub locked: bool,
    pub label: String,
    pub keyframes: Vec<KeyframeMarker>,
    pub group_curtain_up: f32,
    pub group_curtain_down: f32,
    pub clip_curtain_up: f32,
    pub clip_curtain_down: f32,
    pub has_waveform: bool,
    pub waveform_x: f32,
    pub waveform_width: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeyframeMarker {
    pub frame: i32,
    pub x: f32,
}

impl TimelineWindow {
    pub fn clip_view_model(&self, obj: &TimelineObject, locked: bool) -> ClipViewModel {
        let height = LAYER_HEIGHT * 0.75;
        let x = self.frame_to_x(obj.start_frame);
        let y = self.layer_to_y(obj.layer) + (LAYER_HEIGHT - height) * 0.5;
        let width = ((obj.end_frame - obj.start_frame) as f32 * self.zoom_scale).max(4.0);
        let span = (obj.end_frame - obj.start_frame).max(1) as f32;
        let keyframes = obj
            .keyframe_frames
            .iter()
            .map(|&frame| KeyframeMarker {
                frame,
                x: x + (frame - obj.start_frame) as f32 * width / span,
            })
            .collect();
        ClipViewModel {
            id: obj.id,
            x,
            y,
            width,
            height,
            color_index: obj.kind.max(0),
            kind_known: obj.kind_known,
            selected: obj.selected,
            locked,
            label: obj.label.clone(),
            keyframes,
            group_curtain_up: obj.group_layer_count_up as f32 * LAYER_HEIGHT,
            group_curtain_down: obj.group_layer_count_down as f32 * LAYER_HEIGHT,
            clip_curtain_up: obj.clip_layer_count_up as f32 * LAYER_HEIGHT,
            clip_curtain_down: obj.clip_layer_count_down as f32 * LAYER_HEIGHT,
            has_waveform: obj.has_waveform && self.show_waveform,
            waveform_x: x
                + 3.0
                + (obj.waveform_origin_frame - obj.start_frame) as f32 * self.zoom_scale,
            waveform_width: (obj.waveform_duration_frames as f32 * self.zoom_scale).max(1.0),
        }
    }

    pub fn clip_view_models(&self, state: &SharedAppState) -> Vec<ClipViewModel> {
        let layer_states = {
            let world_holder = app_state::active_world(state);
            let world = world_holder.lock().unwrap();
            world.layer_states()
        };
        self.get_timeline_objects(state)
            .iter()
            .map(|obj| {
                let locked = layer_states.get(obj.layer as usize).is_some_and(|s| s.1);
                self.clip_view_model(obj, locked)
            })
            .collect()
    }

    pub fn hit_test_edge(&self, clip: &ClipViewModel, x_px: f32) -> EdgeHit {
        if x_px < clip.x || x_px > clip.x + clip.width {
            return EdgeHit::None;
        }
        if x_px < clip.x + HANDLE_WIDTH {
            EdgeHit::Left
        } else if x_px > clip.x + clip.width - HANDLE_WIDTH {
            EdgeHit::Right
        } else {
            EdgeHit::Body
        }
    }

    pub fn resize_clip(
        &mut self,
        state: &SharedAppState,
        id: i32,
        start_frame: i32,
        end_frame: i32,
    ) {
        if end_frame <= start_frame {
            return;
        }
        let world_holder = app_state::active_world(state);
        app_state::snapshot_before_edit(state);
        let mut world = world_holder.lock().unwrap();
        world.resize_object(id as usize, start_frame, end_frame);
    }

    pub fn toggle_select(&mut self, state: &SharedAppState, id: i32) {
        if self.selected_ids.contains(&id) {
            self.selected_ids.remove(&id);
        } else {
            self.selected_ids.insert(id);
        }
        let ids = self.selected_ids.iter().map(|&i| i as usize).collect();
        let world_holder = app_state::active_world(state);
        world_holder.lock().unwrap().set_selected_ids(ids);
    }
}
