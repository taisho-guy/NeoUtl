use super::{LAYER_HEIGHT, TimelineWindow};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Slint `timeline-grid.slint` 相当。
/// 各レイヤー行の背景縞(偶数行のみ)と罫線(全行)、フレーム目盛の縦線を描画する。
impl TimelineWindow {
    pub(super) fn draw_grid(&self, painter: &Painter, rect: Rect, layer_count: i32) {
        let frame_interval = self.frame_interval();
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
            (rect.width() / (self.zoom_scale * frame_interval.max(1) as f32)).ceil() as i32 + 1
        } else {
            0
        };
        let first_visible =
            (self.scroll_x / self.zoom_scale / frame_interval.max(1) as f32).floor() as i32;
        for i in 0..line_count {
            let frame = (first_visible + i) * frame_interval;
            let x = rect.min.x + self.frame_to_x(frame);
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            );
        }
    }
}
