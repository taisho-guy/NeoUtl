use super::{HEADER_WIDTH, RULER_HEIGHT, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::ui::preview::PreviewPanel;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

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
        let visuals = ui.visuals().clone();
        let bg = visuals.panel_fill;
        let header_bg = visuals.faint_bg_color;
        let text_color = visuals.text_color();
        let weak_text = visuals.weak_text_color();
        let tick_major = visuals.text_color();
        let tick_minor = visuals.weak_text_color();
        let accent = visuals.selection.bg_fill;

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), RULER_HEIGHT),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, bg);

        let header_rect = Rect::from_min_size(rect.min, Vec2::new(HEADER_WIDTH, RULER_HEIGHT));
        painter.rect_filled(header_rect, 0.0, header_bg);
        let mut zoom_percent = (self.zoom_scale * 100.0).round() as i32;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(header_rect));
        let inner = child.add_sized(
            header_rect.size(),
            egui::DragValue::new(&mut zoom_percent)
                .range(10..=400)
                .suffix("%"),
        );
        if inner.changed() {
            let new_scale = (zoom_percent as f32 / 100.0).clamp(0.1, 4.0);
            self.zoom_scale = new_scale;
            app_state::active_world(state)
                .lock()
                .unwrap()
                .set_zoom(new_scale);
        }

        let body_rect =
            Rect::from_min_max(Pos2::new(rect.min.x + HEADER_WIDTH, rect.min.y), rect.max);
        let grid_interval = {
            let world_holder = app_state::active_world(state);
            let world = world_holder.lock().unwrap();
            let active_scene = world.active_scene();
            world
                .scenes()
                .into_iter()
                .find(|scene| scene.id == active_scene)
                .map_or(30, |scene| scene.effective_grid_interval())
        };
        let start_tick =
            (self.scroll_x / self.zoom_scale / grid_interval as f32).floor() as i32 * grid_interval;
        let tick_count =
            (body_rect.width() / self.zoom_scale / grid_interval as f32).ceil() as i32 + 2;
        for i in 0..tick_count {
            let frame = start_tick + i * grid_interval;
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
                Stroke::new(1.0, if is_second { tick_major } else { tick_minor }),
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
                if is_second { text_color } else { weak_text },
            );
        }
        let playhead_x = body_rect.min.x + self.frame_to_x(current_frame);
        painter.line_segment(
            [
                Pos2::new(playhead_x, rect.min.y),
                Pos2::new(playhead_x, rect.max.y),
            ],
            Stroke::new(2.0, accent),
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
