use super::context_menu::menu_row;
use super::util::build_layer_menu;
use super::{MenuState, TimelineWindow};
use crate::app_state::SharedAppState;
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{ContextMenuItem, LayerState};
use egui::{Pos2, Rect};
use std::cell::RefCell;
use std::rc::Rc;

const MENU_WIDTH: f32 = 220.0;

impl TimelineWindow {
    pub(super) fn open_layer_menu(&mut self, pos: Pos2, layer: i32, layer_states: &[LayerState]) {
        let states: Vec<(bool, bool)> =
            layer_states.iter().map(|s| (s.visible, s.locked)).collect();
        let items = build_layer_menu(layer, &states, self.show_grid, self.show_waveform);
        self.layer_menu = Some(MenuState {
            pos,
            hit_id: -2,
            frame: 0,
            layer,
            items,
            open_submenu: None,
            just_opened: true,
        });
    }

    pub(super) fn layer_menu_layer(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        let Some(mut menu) = self.layer_menu.take() else {
            return;
        };
        let mut keep_open = true;
        let mut fire: Option<ContextMenuItem> = None;
        let mut row_rects: Vec<Rect> = Vec::with_capacity(menu.items.len());

        let menu_area = egui::Area::new(ui.id().with("timeline-layer-menu"))
            .fixed_pos(menu.pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        ui.set_width(MENU_WIDTH);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for item in &menu.items {
                            if item.action == 4 {
                                ui.add_space(2.0);
                                ui.separator();
                                ui.add_space(2.0);
                                row_rects.push(Rect::NOTHING);
                                continue;
                            }
                            let has_submenu = !item.submenu.is_empty() || item.action == 17;
                            let resp = menu_row(ui, item, has_submenu);
                            row_rects.push(resp.rect);
                            if resp.hovered() && item.enabled && has_submenu {
                                menu.open_submenu = row_rects.len().checked_sub(1);
                            }
                            if resp.clicked() && item.enabled {
                                if has_submenu {
                                    let idx = row_rects.len() - 1;
                                    menu.open_submenu = if menu.open_submenu == Some(idx) {
                                        None
                                    } else {
                                        Some(idx)
                                    };
                                } else {
                                    fire = Some(item.clone());
                                }
                            }
                        }
                    });
            })
            .response;
        let mut occupied_rect = menu_area.rect;

        if let Some(idx) = menu.open_submenu {
            if let (Some(anchor), Some(item)) = (row_rects.get(idx), menu.items.get(idx)) {
                if !item.submenu.is_empty() {
                    let sub_pos = Pos2::new(anchor.max.x, anchor.min.y);
                    let submenu_area = egui::Area::new(ui.id().with("timeline-layer-submenu"))
                        .fixed_pos(sub_pos)
                        .order(egui::Order::Foreground)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style())
                                .inner_margin(egui::Margin::symmetric(0, 4))
                                .show(ui, |ui| {
                                    ui.set_width(MENU_WIDTH);
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    for sub in &item.submenu {
                                        let resp = menu_row(ui, sub, false);
                                        if resp.clicked() && sub.enabled {
                                            fire = Some(sub.clone());
                                        }
                                    }
                                });
                        })
                        .response;
                    occupied_rect = occupied_rect.union(submenu_area.rect);
                }
            }
        }

        if let Some(item) = fire {
            self.apply_menu_action(state, preview_panel, dialogs, &menu, &item, 0);
            keep_open = false;
        }

        let just_opened = menu.just_opened;
        menu.just_opened = false;

        if keep_open
            && !just_opened
            && ui.input(|i| i.pointer.any_click())
            && ui
                .ctx()
                .pointer_interact_pos()
                .map_or(false, |p| !occupied_rect.contains(p))
        {
            keep_open = false;
        }
        if keep_open {
            self.layer_menu = Some(menu);
        }
    }
}
