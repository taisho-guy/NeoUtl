use super::{HEADER_WIDTH, LAYER_HEIGHT, TimelineWindow};
use crate::app_state::SharedAppState;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::LayerState;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use egui_material_icons::icons;
use std::cell::RefCell;
use std::rc::Rc;

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
        let visuals = ui.visuals().clone();
        let base_bg = visuals.panel_fill;
        let stripe_bg = visuals.faint_bg_color;
        let hidden_bg = visuals.extreme_bg_color;
        let locked_bg = visuals.warn_fg_color.gamma_multiply(0.25);
        let selected_bg = visuals.selection.bg_fill;
        let separator = visuals.widgets.noninteractive.bg_stroke.color;
        let text_normal = visuals.text_color();
        let text_weak = visuals.weak_text_color();
        let text_locked = visuals.warn_fg_color;
        let text_selected = visuals.strong_text_color();

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(HEADER_WIDTH, content_height),
            Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, base_bg);

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
            let bg = if selected {
                selected_bg
            } else if !ls.visible {
                hidden_bg
            } else if ls.locked {
                locked_bg
            } else if i % 2 == 0 {
                stripe_bg
            } else {
                base_bg
            };
            painter.rect_filled(row, 0.0, bg);
            if selected {
                painter.rect_stroke(
                    row,
                    0.0,
                    Stroke::new(2.0, selected_bg.gamma_multiply(1.4)),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.line_segment(
                    [
                        Pos2::new(row.min.x, row.max.y),
                        Pos2::new(row.max.x, row.max.y),
                    ],
                    Stroke::new(1.0, separator),
                );
            }
            let text_color = if !ls.visible {
                text_weak
            } else if ls.locked {
                text_locked
            } else if selected {
                text_selected
            } else {
                text_normal
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
                if let Some(pos) = select_resp.interact_pointer_pos() {
                    self.open_layer_menu(pos, i, layer_states);
                }
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
                    <&str>::from(icons::ICON_LOCK),
                    egui::FontId::proportional(9.0),
                    text_locked,
                );
            }
            if !ls.visible {
                painter.text(
                    Pos2::new(vis_rect.min.x + 2.0, vis_rect.max.y - 12.0),
                    egui::Align2::LEFT_TOP,
                    <&str>::from(icons::ICON_VISIBILITY_OFF),
                    egui::FontId::proportional(8.0),
                    text_weak,
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
