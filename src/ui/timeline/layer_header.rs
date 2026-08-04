use super::{HEADER_WIDTH, LAYER_HEIGHT, TimelineWindow};
use crate::app_state::SharedAppState;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::LayerState;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

/// Slint `timeline/layer-header.slint` 相当。
impl TimelineWindow {
    pub(super) fn layer_header(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        layer_count: i32,
        layer_states: &[LayerState],
        content_height: f32,
    ) {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(HEADER_WIDTH, content_height),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x20, 0x20, 0x28));

        for i in 0..layer_count {
            let y = rect.min.y + self.layer_to_y(i);
            if y + LAYER_HEIGHT < rect.min.y || y > rect.max.y {
                continue;
            }
            let default_state = LayerState {
                visible: true,
                locked: false,
            };
            let ls = layer_states
                .get(i as usize)
                .copied()
                .unwrap_or(default_state);
            let selected = i == self.selected_layer;
            let row = Rect::from_min_size(
                Pos2::new(rect.min.x, y),
                Vec2::new(HEADER_WIDTH, LAYER_HEIGHT),
            );
            let bg = if !ls.visible {
                Color32::from_rgb(0x16, 0x16, 0x1a)
            } else if ls.locked {
                Color32::from_rgb(0x4a, 0x2a, 0x2a)
            } else if selected {
                Color32::from_rgb(0x35, 0x35, 0x4a)
            } else if i % 2 == 0 {
                Color32::from_rgb(0x24, 0x24, 0x2c)
            } else {
                Color32::from_rgb(0x20, 0x20, 0x28)
            };
            painter.rect_filled(row, 0.0, bg);
            painter.rect_stroke(
                row,
                0.0,
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        Color32::from_rgb(0x6a, 0x8f, 0xff)
                    } else {
                        Color32::from_rgb(0x2a, 0x2a, 0x32)
                    },
                ),
                egui::StrokeKind::Outside,
            );
            let text_color = if !ls.visible {
                Color32::from_rgb(0x55, 0x55, 0x60)
            } else if ls.locked {
                Color32::from_rgb(0xff, 0xb0, 0xb0)
            } else if selected {
                Color32::WHITE
            } else {
                Color32::from_rgb(0xcc, 0xcc, 0xd6)
            };
            painter.text(
                row.center(),
                egui::Align2::CENTER_CENTER,
                (i + 1).to_string(),
                egui::FontId::proportional(11.0),
                text_color,
            );

            let select_id = ui.id().with(("layer-select", i));
            let select_rect =
                Rect::from_min_size(row.min, Vec2::new(HEADER_WIDTH - 16.0, LAYER_HEIGHT));
            let select_resp = ui.interact(select_rect, select_id, Sense::click());
            if select_resp.clicked() {
                self.selected_layer = i;
            }
            if select_resp.secondary_clicked() {
                self.selected_layer = i;
                self.toggle_layer_locked(state, preview_panel, i);
            }

            let vis_id = ui.id().with(("layer-vis", i));
            let vis_rect = Rect::from_min_size(
                Pos2::new(row.max.x - 16.0, row.min.y),
                Vec2::new(16.0, LAYER_HEIGHT),
            );
            let vis_resp = ui.interact(vis_rect, vis_id, Sense::click());
            if vis_resp.clicked() {
                self.toggle_layer_visible(state, preview_panel, i);
            }
            if ls.locked {
                painter.text(
                    Pos2::new(vis_rect.min.x + 2.0, vis_rect.min.y + 2.0),
                    egui::Align2::LEFT_TOP,
                    "🔒",
                    egui::FontId::proportional(9.0),
                    Color32::from_rgb(0xff, 0xb0, 0xb0),
                );
            }
            if !ls.visible {
                painter.text(
                    Pos2::new(vis_rect.min.x + 2.0, vis_rect.max.y - 12.0),
                    egui::Align2::LEFT_TOP,
                    "—",
                    egui::FontId::proportional(8.0),
                    Color32::from_rgb(0x55, 0x55, 0x60),
                );
            }
        }

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let max_scroll = (layer_count as f32 * LAYER_HEIGHT - content_height).max(0.0);
                self.scroll_y = (self.scroll_y - scroll).clamp(0.0, max_scroll);
            }
        }
    }
}
