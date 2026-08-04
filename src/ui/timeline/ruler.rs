use super::{HEADER_WIDTH, RULER_HEIGHT, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::ui::preview::PreviewPanel;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

/// Slint `timeline/ruler.slint` 相当。目盛描画・スクラブ・ホイールズームを担う。
impl TimelineWindow {
    pub(super) fn ruler(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        current_frame: i32,
        total_frames: i32,
    ) {
        let _ = total_frames;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), RULER_HEIGHT),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x1a, 0x1a, 0x20));

        let header_rect = Rect::from_min_size(rect.min, Vec2::new(HEADER_WIDTH, RULER_HEIGHT));
        painter.rect_filled(header_rect, 0.0, Color32::from_rgb(0x20, 0x20, 0x28));
        painter.text(
            header_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}%", (self.zoom_scale * 100.0).round()),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(0x88, 0x99, 0xaa),
        );

        let body_rect =
            Rect::from_min_max(Pos2::new(rect.min.x + HEADER_WIDTH, rect.min.y), rect.max);
        let frame_interval = self.frame_interval();
        let start_tick = (self.scroll_x / self.zoom_scale / frame_interval as f32).floor() as i32
            * frame_interval;
        let tick_count =
            (body_rect.width() / self.zoom_scale / frame_interval as f32).ceil() as i32 + 2;
        for i in 0..tick_count {
            let frame = start_tick + i * frame_interval;
            let x = body_rect.min.x + self.frame_to_x(frame);
            if x < body_rect.min.x || x > body_rect.max.x {
                continue;
            }
            let is_second = frame % 30.max(1) == 0;
            let h = if is_second {
                rect.height()
            } else {
                rect.height() * 0.5
            };
            painter.line_segment(
                [Pos2::new(x, rect.max.y), Pos2::new(x, rect.max.y - h)],
                Stroke::new(
                    1.0,
                    if is_second {
                        Color32::from_rgb(0x88, 0x99, 0xaa)
                    } else {
                        Color32::from_rgb(0x45, 0x45, 0x4e)
                    },
                ),
            );
            let label = if is_second {
                format!("{}s", frame / 30.max(1))
            } else {
                frame.to_string()
            };
            painter.text(
                Pos2::new(x + 3.0, rect.min.y + 2.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(9.0),
                Color32::from_rgb(0x6a, 0x6a, 0x76),
            );
        }
        let playhead_x = body_rect.min.x + self.frame_to_x(current_frame);
        painter.line_segment(
            [
                Pos2::new(playhead_x, rect.min.y),
                Pos2::new(playhead_x, rect.max.y),
            ],
            Stroke::new(2.0, Color32::from_rgb(0xff, 0x45, 0x00)),
        );

        if response.hovered() {
            if response.dragged() || response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let frame = self.px_to_frame(pos.x - body_rect.min.x);
                    self.seek(state, preview_panel, frame);
                }
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let anchor_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(body_rect.min);
                let anchor_frame = self.px_to_frame(anchor_pos.x - body_rect.min.x);
                let new_scale = if scroll > 0.0 {
                    self.zoom_scale * 1.1
                } else {
                    self.zoom_scale * 0.9
                }
                .clamp(0.1, 10.0);
                self.scroll_x =
                    (self.scroll_x + (new_scale - self.zoom_scale) * anchor_frame as f32).max(0.0);
                self.zoom_scale = new_scale;
                app_state::active_world(state)
                    .lock()
                    .unwrap()
                    .set_zoom(new_scale);
            }
        }
    }
}
