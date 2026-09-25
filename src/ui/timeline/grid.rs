use super::{LAYER_HEIGHT, TimelineWindow};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

impl TimelineWindow {
    pub(super) fn draw_grid(
        &self,
        painter: &Painter,
        rect: Rect,
        layer_count: i32,
        grid_interval: i32,
    ) {
        let frame_interval = grid_interval.max(1);
        for i in 0..layer_count {
            let y = rect.min.y + self.layer_to_y(i);
            if y + LAYER_HEIGHT < rect.min.y || y > rect.max.y {
                continue;
            }
            let row = Rect::from_min_size(
                Pos2::new(rect.min.x, y),
                Vec2::new(rect.width(), LAYER_HEIGHT),
            );
            if i % 2 == 0 {
                painter.rect_filled(row, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 5));
            }
            painter.line_segment(
                [
                    Pos2::new(row.min.x, row.max.y),
                    Pos2::new(row.max.x, row.max.y),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 13)),
            );
        }

        let line_count = if self.zoom_scale > 0.0 {
            f32_to_i32((rect.width() / (self.zoom_scale * i32_to_f32(frame_interval))).ceil())
                .saturating_add(1)
        } else {
            0
        };
        let first_visible =
            f32_to_i32((self.scroll_x / self.zoom_scale / i32_to_f32(frame_interval)).floor());
        for i in 0..line_count {
            let frame = first_visible
                .saturating_add(i)
                .saturating_mul(frame_interval);
            let x = rect.min.x + self.frame_to_x(frame);
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            );
        }
    }
}

fn i32_to_f32(value: i32) -> f32 {
    value.to_string().parse().unwrap_or_else(|_| {
        if value.is_negative() {
            f32::MIN
        } else {
            f32::MAX
        }
    })
}

fn f32_to_i32(value: f32) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.round().to_string().parse().unwrap_or_else(|_| {
        if value.is_sign_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}
