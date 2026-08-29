use super::util::build_context_menu;
use super::{DragMode, MenuState, TimelineWindow};
use crate::app_state::{self, SharedAppState};
use crate::objects::registry;
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{ContextMenuItem, ObjectKindItem};
use egui::{Pos2, Rect};
use egui_material_icons::icons;
use std::cell::RefCell;
use std::rc::Rc;

const MENU_WIDTH: f32 = 220.0;
const ROW_HEIGHT: f32 = 24.0;

fn icon_glyph(icon: &str) -> &'static str {
    match icon {
        "scissors" => icons::ICON_CONTENT_CUT.into(),
        "copy" => icons::ICON_CONTENT_COPY.into(),
        "paste" => icons::ICON_CONTENT_PASTE.into(),
        "trash" => icons::ICON_DELETE.into(),
        "copy-plus" => icons::ICON_LIBRARY_ADD.into(),
        "circle-plus" => icons::ICON_ADD_CIRCLE.into(),
        "mouse-pointer-click" => icons::ICON_ADS_CLICK.into(),
        "list" => icons::ICON_LIST.into(),
        "grid" => icons::ICON_GRID_ON.into(),
        "audio-lines" => icons::ICON_GRAPHIC_EQ.into(),
        "eye" => icons::ICON_VISIBILITY.into(),
        "lock" => icons::ICON_LOCK.into(),
        "settings" => icons::ICON_SETTINGS.into(),
        _ => icons::ICON_CIRCLE.into(),
    }
}

pub(super) fn menu_row(
    ui: &mut egui::Ui,
    item: &ContextMenuItem,
    has_submenu: bool,
) -> egui::Response {
    let desired = egui::vec2(ui.available_width(), ROW_HEIGHT);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let resp = if item.enabled {
        resp
    } else {
        resp.on_hover_cursor(egui::CursorIcon::NotAllowed)
    };
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        if item.enabled && resp.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, visuals.selection.bg_fill.gamma_multiply(0.55));
        }
        let text_color = if item.enabled {
            visuals.text_color()
        } else {
            visuals.weak_text_color()
        };
        let check_x = rect.min.x + 8.0;
        if let Some(checked) = item.checked {
            if checked {
                ui.painter().text(
                    egui::pos2(check_x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    <&str>::from(icons::ICON_CHECK),
                    egui::FontId::proportional(13.0),
                    text_color,
                );
            }
        }
        let icon_x = if item.checked.is_some() {
            check_x + 16.0
        } else {
            rect.min.x + 10.0
        };
        let label_x = if item.icon.is_empty() {
            icon_x
        } else {
            ui.painter().text(
                egui::pos2(icon_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                icon_glyph(&item.icon),
                egui::FontId::proportional(12.0),
                text_color.gamma_multiply(0.75),
            );
            icon_x + 18.0
        };
        ui.painter().text(
            egui::pos2(label_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &item.label,
            egui::FontId::proportional(13.0),
            text_color,
        );
        if has_submenu {
            ui.painter().text(
                egui::pos2(rect.max.x - 10.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                <&str>::from(icons::ICON_CHEVRON_RIGHT),
                egui::FontId::proportional(13.0),
                text_color,
            );
        }
    }
    resp
}

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
                name: crate::localization::object_name(&plugin.name),
            })
            .collect();
        let registry_snapshot = registry();
        let objects: Vec<(i32, String)> = app_state::active_world(state)
            .lock()
            .unwrap()
            .get_timeline_objects()
            .iter()
            .map(|o| {
                let label = registry_snapshot.get(o.kind as usize).map_or_else(
                    || crate::localization::tr("Unknown"),
                    |p| crate::localization::object_name(&p.name),
                );
                (o.id, format!("[{}] {}", o.id, label))
            })
            .collect();
        let items = build_context_menu(
            hit_id,
            clipboard_empty,
            &kinds,
            &objects,
            self.show_grid,
            self.show_waveform,
            self.select_range,
        );
        let _ = ui;
        self.menu = Some(MenuState {
            pos,
            hit_id,
            frame,
            layer,
            items,
            open_submenu: None,
            just_opened: true,
        });
    }

    pub(super) fn context_menu_layer(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        dialogs: &Rc<RefCell<DialogSet>>,
        current_frame: i32,
        _kinds: &[ObjectKindItem],
    ) {
        self.finish_drag_if_released(ui, state, preview_panel);

        let Some(mut menu) = self.menu.take() else {
            return;
        };
        let mut keep_open = true;
        let mut fire: Option<ContextMenuItem> = None;
        let mut row_rects: Vec<Rect> = Vec::with_capacity(menu.items.len());

        let menu_area = egui::Area::new(ui.id().with("timeline-context-menu"))
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
                    let submenu_area = egui::Area::new(ui.id().with("timeline-context-submenu"))
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
            self.apply_menu_action(state, preview_panel, dialogs, &menu, &item, current_frame);
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
            self.menu = Some(menu);
        }
    }

    pub(super) fn apply_menu_action(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        dialogs: &Rc<RefCell<DialogSet>>,
        menu: &MenuState,
        item: &ContextMenuItem,
        current_frame: i32,
    ) {
        match item.action {
            0 => {
                let ids = self.selection_target_ids(menu.hit_id);
                self.split_objects_at(state, preview_panel, &ids, current_frame);
            }
            1 => {
                let ids = self.selection_target_ids(menu.hit_id);
                self.delete_objects(state, preview_panel, &ids);
            }
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
            12 => self.cut_selection_range(state, preview_panel),
            13 => self.cut_selection_range_and_pack(state, preview_panel),
            14 => self.select_object(state, &(), item.kind, false),
            15 => self.show_grid = !self.show_grid,
            16 => self.show_waveform = !self.show_waveform,
            18 => {
                let ids = self.selection_target_ids(menu.hit_id);
                self.pack_left(state, preview_panel, &ids);
            }
            19 => {
                let ids = self.selection_target_ids(menu.hit_id);
                self.cut_and_pack(state, preview_panel, &ids);
            }
            20 => {
                let ids = self.selection_target_ids(menu.hit_id);
                self.extract_selection(state, preview_panel, &ids);
            }
            53 => self.select_range = None,
            40 => self.toggle_layer_locked(state, preview_panel, item.kind),
            41 => self.toggle_layer_visible(state, preview_panel, item.kind),
            50 => {
                let scene_id = app_state::active_world(state)
                    .lock()
                    .unwrap()
                    .active_scene();
                dialogs.borrow_mut().open_scene_edit(state, scene_id);
            }
            51 => dialogs.borrow_mut().project_settings.open(state),
            52 => dialogs.borrow_mut().system_settings.open = true,
            _ => {}
        }
    }
}
