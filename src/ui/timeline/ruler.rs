use super::{HEADER_WIDTH, RULER_HEIGHT, TimelineWindow};
use crate::app::state::{self as app_state, SharedAppState};
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
        let mut zoom_percent = f32_to_i32(self.zoom_scale * 100.0);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(header_rect));
        let inner = child.add_sized(
            header_rect.size(),
            egui::DragValue::new(&mut zoom_percent)
                .range(10..=400)
                .suffix("%"),
        );
        if inner.changed() {
            let new_scale = (i32_to_f32(zoom_percent) / 100.0).clamp(0.1, 4.0);
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
            f32_to_i32((self.scroll_x / self.zoom_scale / i32_to_f32(grid_interval)).floor())
                .saturating_mul(grid_interval);
        let tick_count =
            f32_to_i32((body_rect.width() / self.zoom_scale / i32_to_f32(grid_interval)).ceil())
                .saturating_add(2);
        for i in 0..tick_count {
            let frame = start_tick.saturating_add(i.saturating_mul(grid_interval));
            let x = body_rect.min.x + self.frame_to_x(frame);
            if x < body_rect.min.x || x > body_rect.max.x {
                continue;
            }
            let is_second = frame % 30 == 0;
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
                format!("{}s", frame / 30)
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

        self.handle_ruler_pointer(&response, body_rect, state, preview_panel);

        self.handle_ruler_zoom(ui, body_rect, state);
    }

    fn handle_ruler_pointer(
        &mut self,
        response: &egui::Response,
        body_rect: Rect,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.select_range_anchor = Some(self.px_to_frame(pos.x - body_rect.min.x).max(0));
        }
        if response.dragged()
            && let (Some(anchor_frame), Some(pos)) =
                (self.select_range_anchor, response.interact_pointer_pos())
        {
            let current_frame = self.px_to_frame(pos.x - body_rect.min.x).max(0);
            self.select_range = Some((
                anchor_frame.min(current_frame),
                anchor_frame.max(current_frame),
            ));
        } else if response.double_clicked() {
            self.select_range = None;
            self.select_range_anchor = None;
        } else if response.hovered()
            && response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.seek(
                state,
                preview_panel,
                self.px_to_frame(pos.x - body_rect.min.x),
            );
        }
    }

    fn handle_ruler_zoom(&mut self, ui: &egui::Ui, body_rect: Rect, state: &SharedAppState) {
        if !ui
            .input(|input| input.pointer.hover_pos())
            .is_some_and(|pos| body_rect.contains(pos))
        {
            return;
        }
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll == 0.0 {
            return;
        }
        let anchor_pos = ui
            .input(|input| input.pointer.hover_pos())
            .unwrap_or(body_rect.min);
        let anchor_frame = self.px_to_frame(anchor_pos.x - body_rect.min.x);
        let new_scale = if scroll > 0.0 {
            self.zoom_scale * 1.1
        } else {
            self.zoom_scale * 0.9
        }
        .clamp(0.1, 10.0);
        self.scroll_x =
            (self.scroll_x + (new_scale - self.zoom_scale) * i32_to_f32(anchor_frame)).max(0.0);
        self.zoom_scale = new_scale;
        app_state::active_world(state)
            .lock()
            .unwrap()
            .set_zoom(new_scale);
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
