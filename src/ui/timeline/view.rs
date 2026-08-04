use super::{RangeSelect, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{LayerState, TimelineObject};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

/// Slint `timeline/timeline-view.slint` 相当。
/// グリッド(grid.rs)・クリップ(clip_item.rs)を合成し、背景ドラッグ選択と
/// 右クリックメニュー起動(open_context_menu, context_menu.rs)を仲介する。
impl TimelineWindow {
    pub(super) fn timeline_view(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
        current_frame: i32,
        total_frames: i32,
        layer_count: i32,
        objects: &[TimelineObject],
        layer_states: &[LayerState],
        content_height: f32,
    ) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), content_height),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x17, 0x17, 0x1b));

        self.draw_grid(&painter, rect, layer_count);

        self.handle_background_input(ui, &response, state, preview_panel);

        if let Some(range) = &self.range {
            let a = rect.min + range.anchor.to_vec2();
            let c = rect.min + range.cur.to_vec2();
            let sel = Rect::from_two_pos(a, c);
            painter.rect_filled(
                sel,
                0.0,
                Color32::from_rgba_unmultiplied(0x4a, 0x8f, 0xff, 0x33),
            );
            painter.rect_stroke(
                sel,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(0x4a, 0x8f, 0xff)),
                egui::StrokeKind::Outside,
            );
        }

        for obj in objects {
            let locked = layer_states
                .get(obj.layer as usize)
                .map_or(false, |s| s.locked);
            self.clip_ui(
                ui,
                &painter,
                rect,
                state,
                preview_panel,
                &(),
                obj,
                locked,
                total_frames,
            );
        }

        let playhead_x = rect.min.x + self.frame_to_x(current_frame);
        painter.line_segment(
            [
                Pos2::new(playhead_x, rect.min.y),
                Pos2::new(playhead_x, rect.max.y),
            ],
            Stroke::new(2.0, Color32::from_rgba_unmultiplied(0xff, 0x45, 0x00, 0x88)),
        );

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let max_scroll = (total_frames as f32 * self.zoom_scale - rect.width()).max(0.0);
                self.scroll_x = (self.scroll_x - scroll).clamp(0.0, max_scroll);
            }
        }
    }

    pub(super) fn handle_background_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        if self.drag.is_some() || self.kdrag.is_some() {
            return;
        }
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = pos - response.rect.min;
                self.seek(state, preview_panel, self.px_to_frame(local.x));
                self.range = Some(RangeSelect {
                    anchor: Pos2::new(local.x, local.y),
                    cur: Pos2::new(local.x, local.y),
                });
            }
        }
        if response.dragged() && self.range.is_some() {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = pos - response.rect.min;
                if let Some(range) = &mut self.range {
                    range.cur = Pos2::new(local.x, local.y);
                }
            }
        }
        if response.drag_stopped() {
            if let Some(range) = self.range.take() {
                if (range.cur.x - range.anchor.x).abs() > 3.0
                    || (range.cur.y - range.anchor.y).abs() > 3.0
                {
                    let start_frame = self.px_to_frame(range.anchor.x.min(range.cur.x));
                    let end_frame = self.px_to_frame(range.anchor.x.max(range.cur.x));
                    let start_layer = self.px_to_layer(range.anchor.y.min(range.cur.y));
                    let end_layer = self.px_to_layer(range.anchor.y.max(range.cur.y));
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
                }
            }
        }
        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = pos - response.rect.min;
                self.open_context_menu(
                    ui,
                    state,
                    pos,
                    self.px_to_frame(local.x),
                    self.px_to_layer(local.y),
                    -1,
                );
            }
        }
    }
}
