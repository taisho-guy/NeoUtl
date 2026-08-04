use super::util::build_context_menu;
use super::{DragMode, MenuState, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::objects::registry;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{ContextMenuItem, ObjectKindItem};
use egui::{Pos2, Rect, Vec2};
use std::cell::RefCell;
use std::rc::Rc;

/// Slintトップレベル`timeline.slint`の右クリックメニュー部分に対応する。
/// 項目集合の生成は `util::build_context_menu` に一本化する
/// （区切り線=action4の扱いを含め、生成経路をここと重複させない）。
impl TimelineWindow {
    pub(super) fn finish_drag_if_released(
        &mut self,
        ui: &egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        if self.drag.is_none() {
            return;
        }
        if !ui.input(|i| i.pointer.any_released()) {
            return;
        }
        let drag = self.drag.take().unwrap();
        let exists = app_state::active_world(state)
            .lock()
            .unwrap()
            .object_exists(drag.id as usize);
        if !exists {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        match drag.mode {
            DragMode::Move => {
                if self.ripple_mode {
                    world.ripple_move_object(drag.id as usize, drag.preview_start);
                } else {
                    world.move_object(drag.id as usize, drag.preview_start, drag.preview_layer);
                }
            }
            DragMode::ResizeLeft => {
                world.resize_object(drag.id as usize, drag.preview_start, drag.preview_end);
            }
            DragMode::ResizeRight => {
                if self.ripple_mode {
                    world.ripple_resize_object(drag.id as usize, drag.preview_end);
                } else {
                    world.resize_object(drag.id as usize, drag.preview_start, drag.preview_end);
                }
            }
        }
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn open_context_menu(
        &mut self,
        ui: &egui::Ui,
        state: &SharedAppState,
        pos: Pos2,
        frame: i32,
        layer: i32,
        hit_id: i32,
    ) {
        let clipboard_empty = app_state::clipboard(state).is_empty();
        let kinds: Vec<ObjectKindItem> = registry()
            .iter()
            .enumerate()
            .map(|(kind_id, plugin)| ObjectKindItem {
                kind: kind_id as i32,
                name: plugin.name.clone(),
            })
            .collect();
        let items = build_context_menu(hit_id, self.ripple_mode, clipboard_empty, &kinds);
        let _ = ui;
        self.menu = Some(MenuState {
            pos,
            hit_id,
            frame,
            layer,
            items,
        });
    }

    pub(super) fn context_menu_layer(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        current_frame: i32,
        _kinds: &[ObjectKindItem],
    ) {
        self.finish_drag_if_released(ui, state, preview_panel);

        let Some(menu) = self.menu.take() else { return };
        let mut keep_open = true;
        egui::Area::new(ui.id().with("timeline-context-menu"))
            .fixed_pos(menu.pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for item in &menu.items {
                        if item.action == 4 {
                            ui.separator();
                            continue;
                        }
                        let clicked = ui
                            .add_enabled(item.enabled, egui::Button::new(&item.label))
                            .clicked();
                        if clicked {
                            self.apply_menu_action(
                                state,
                                preview_panel,
                                &menu,
                                item,
                                current_frame,
                            );
                            keep_open = false;
                        }
                    }
                });
            });
        if ui.input(|i| i.pointer.any_click())
            && ui.ctx().pointer_hover_pos().map_or(false, |p| {
                !Rect::from_min_size(
                    menu.pos,
                    Vec2::new(190.0, menu.items.len() as f32 * 24.0 + 8.0),
                )
                .contains(p)
            })
        {
            keep_open = false;
        }
        if keep_open {
            self.menu = Some(menu);
        }
    }

    pub(super) fn apply_menu_action(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        menu: &MenuState,
        item: &ContextMenuItem,
        current_frame: i32,
    ) {
        match item.action {
            0 => self.split_object_at(state, preview_panel, menu.hit_id, current_frame),
            1 => self.delete_object(state, preview_panel, menu.hit_id),
            2 => self.add_object_at(state, preview_panel, menu.frame, menu.layer, item.kind),
            3 => self.ripple_mode = !self.ripple_mode,
            5 => {
                if app_state::undo_active(state) {
                    self.after_structural_edit(state, preview_panel);
                }
            }
            6 => {
                if app_state::redo_active(state) {
                    self.after_structural_edit(state, preview_panel);
                }
            }
            7 => self.duplicate_requested(state, preview_panel, menu.hit_id),
            8 => self.cut_requested(state, preview_panel, menu.hit_id),
            9 => self.copy_requested(state, menu.hit_id),
            10 => self.paste_requested(state, preview_panel),
            _ => {}
        }
    }
}
