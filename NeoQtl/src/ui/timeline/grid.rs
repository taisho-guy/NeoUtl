use super::{LAYER_HEIGHT, TimelineWindow};

#[derive(Clone, Copy, Debug, Default)]
pub struct GridLine {
    pub frame: i32,
    pub x: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayerRow {
    pub index: i32,
    pub y: f32,
    pub height: f32,
    pub shaded: bool,
}

impl TimelineWindow {
    pub fn compute_grid_lines(&self, viewport_width: f32, grid_interval: i32) -> Vec<GridLine> {
        if self.zoom_scale <= 0.0 || viewport_width <= 0.0 {
            return Vec::new();
        }
        let interval = grid_interval.max(1);
        let step_px = self.zoom_scale * interval as f32;
        let count = (viewport_width / step_px).ceil() as i32 + 1;
        let first = (self.scroll_x / step_px).floor() as i32;
        (0..count)
            .map(|i| {
                let frame = (first + i) * interval;
                GridLine {
                    frame,
                    x: self.frame_to_x(frame),
                }
            })
            .collect()
    }

    pub fn compute_layer_rows(&self, layer_count: i32, viewport_height: f32) -> Vec<LayerRow> {
        (0..layer_count.max(0))
            .filter_map(|index| {
                let y = self.layer_to_y(index);
                if y + LAYER_HEIGHT < 0.0 || y > viewport_height {
                    return None;
                }
                Some(LayerRow {
                    index,
                    y,
                    height: LAYER_HEIGHT,
                    shaded: index % 2 == 0,
                })
            })
            .collect()
    }
}
