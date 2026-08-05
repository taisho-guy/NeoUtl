use crate::app_state::{self, SharedAppState};
use crate::ecs::EcsWorld;
use crate::objects::registry;
use crate::shortcuts::{self, CommandId, Scope};
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{ContextMenuItem, LayerState, ObjectKindItem, SceneTabItem, TimelineObject};
use egui::Pos2;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

mod actions;
mod clip_item;
mod context_menu;
mod data;
mod grid;
mod layer_header;
mod ruler;
mod scene_tabs;
mod util;
mod view;

use util::egui_key_name;

const HEADER_WIDTH: f32 = 60.0;
const LAYER_HEIGHT: f32 = 30.0;
const RULER_HEIGHT: f32 = 32.0;
const HANDLE_WIDTH: f32 = 10.0;
const KEYFRAME_SIZE: f32 = 8.0;
const SCENE_TAB_HEIGHT: f32 = 28.0;

#[derive(PartialEq)]
enum DragMode {
    Move,
    ResizeLeft,
    ResizeRight,
}

struct ClipDrag {
    id: i32,
    mode: DragMode,
    press: Pos2,
    start_frame: i32,
    end_frame: i32,
    layer: i32,
    duration: i32,
    preview_start: i32,
    preview_end: i32,
    preview_layer: i32,
}

struct KeyframeDrag {
    id: i32,
    frame: i32,
    press: Pos2,
    delta_frames: i32,
}

struct RangeSelect {
    anchor: Pos2,
    cur: Pos2,
}

struct MenuState {
    pos: Pos2,
    hit_id: i32,
    frame: i32,
    layer: i32,
    items: Vec<ContextMenuItem>,
}

/// 拡張編集ウィンドウ。egui::Painter直接描画によるゼロコピー非対象領域。
/// obj/layer-states/scene-tabsは毎フレームworldから再構築する（イミディエイトモード方式）。
/// zoom-scale/scroll-x/scroll-y/選択状態のみ本構造体へ永続化する。
pub struct TimelineWindow {
    pub open: bool,
    zoom_scale: f32,
    selected_layer: i32,
    ripple_mode: bool,
    scroll_x: f32,
    scroll_y: f32,
    selected_ids: HashSet<i32>,
    last_generation: u64,
    drag: Option<ClipDrag>,
    kdrag: Option<KeyframeDrag>,
    range: Option<RangeSelect>,
    menu: Option<MenuState>,
    waveform_cache: HashMap<PathBuf, egui::TextureHandle>,
}

impl TimelineWindow {
    pub fn new() -> Self {
        Self {
            open: false,
            zoom_scale: 1.0,
            selected_layer: 0,
            ripple_mode: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            selected_ids: HashSet::new(),
            last_generation: 0,
            drag: None,
            kdrag: None,
            range: None,
            menu: None,
            waveform_cache: HashMap::new(),
        }
    }

    pub(super) fn reset_session(&mut self, world: &EcsWorld) {
        self.selected_ids.clear();
        self.selected_layer = 0;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.zoom_scale = world.zoom();
        self.drag = None;
        self.kdrag = None;
        self.range = None;
        self.menu = None;
    }

    pub(super) fn frame_interval(&self) -> i32 {
        if self.zoom_scale > 3.0 {
            10
        } else if self.zoom_scale > 1.0 {
            30
        } else if self.zoom_scale > 0.3 {
            60
        } else {
            300
        }
    }

    pub(super) fn frame_to_x(&self, frame: i32) -> f32 {
        frame as f32 * self.zoom_scale - self.scroll_x
    }

    pub(super) fn px_to_frame(&self, px: f32) -> i32 {
        ((px + self.scroll_x) / self.zoom_scale).floor().max(0.0) as i32
    }

    pub(super) fn layer_to_y(&self, layer: i32) -> f32 {
        layer as f32 * LAYER_HEIGHT - self.scroll_y
    }

    pub(super) fn px_to_layer(&self, py: f32) -> i32 {
        ((py + self.scroll_y) / LAYER_HEIGHT).floor().max(0.0) as i32
    }

    pub fn show(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        if std::mem::take(&mut preview_panel.borrow_mut().open_timeline) {
            self.open = true;
        }
        if !self.open {
            return;
        }

        let generation = preview_panel.borrow().session_generation();
        if generation != self.last_generation {
            self.last_generation = generation;
            let world_holder = app_state::active_world(state);
            let world = world_holder.lock().unwrap();
            self.reset_session(&world);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.body(ui, state, preview_panel, &(), dialogs);
        });
    }

    pub(super) fn body(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        self.handle_shortcuts(ui, state, preview_panel, &());

        let world_holder = app_state::active_world(state);
        let (fps, total_frames, layer_count, layer_states, objects, scene_tabs, kinds) = {
            let world = world_holder.lock().unwrap();
            let fps = world.get_project().fps as f64;
            let selected = self.selected_ids.clone();
            let objects: Vec<TimelineObject> = world
                .get_timeline_objects()
                .iter()
                .map(|data| {
                    let mut o = self.to_egui(ui.ctx(), data, fps);
                    o.selected = selected.contains(&o.id);
                    o
                })
                .collect();
            let layer_states: Vec<LayerState> = world
                .layer_states()
                .iter()
                .map(|&(visible, locked)| LayerState { visible, locked })
                .collect();
            let active_scene = world.active_scene();
            let scene_tabs: Vec<SceneTabItem> = world
                .scenes()
                .iter()
                .map(|s| SceneTabItem {
                    id: s.id,
                    name: s.name.clone(),
                    active: s.id == active_scene,
                })
                .collect();
            let kinds: Vec<ObjectKindItem> = registry()
                .iter()
                .enumerate()
                .map(|(kind_id, plugin)| ObjectKindItem {
                    kind: kind_id as i32,
                    name: crate::localization::object_name(&plugin.name),
                })
                .collect();
            (
                fps,
                world.total_frames(),
                world.layer_count(),
                layer_states,
                objects,
                scene_tabs,
                kinds,
            )
        };
        let _ = fps;
        let current_frame = world_holder.lock().unwrap().current_frame();

        self.scene_tab_bar(ui, state, preview_panel, &scene_tabs, dialogs);
        self.ruler(ui, state, preview_panel, current_frame, total_frames);

        let content_height = ui.available_height();
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            self.layer_header(
                ui,
                state,
                preview_panel,
                layer_count,
                &layer_states,
                content_height,
            );
            self.timeline_view(
                ui,
                state,
                preview_panel,
                &(),
                current_frame,
                total_frames,
                layer_count,
                &objects,
                &layer_states,
                content_height,
            );
        });

        self.context_menu_layer(ui, state, preview_panel, current_frame, &kinds);
    }

    pub(super) fn handle_shortcuts(
        &mut self,
        ui: &egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
    ) {
        let (ctrl, shift, alt, keys) = ui.input(|i| {
            (
                i.modifiers.ctrl,
                i.modifiers.shift,
                i.modifiers.alt,
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key {
                            key, pressed: true, ..
                        } => Some(egui_key_name(*key)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        for key in keys {
            let Some(cmd) = shortcuts::resolve_active(Scope::Timeline, ctrl, shift, alt, &key)
            else {
                continue;
            };
            let hit_id = self.selected_ids.iter().next().copied().unwrap_or(-1);
            match cmd {
                CommandId::DeleteSelected => {
                    if hit_id >= 0 {
                        self.delete_object(state, preview_panel, hit_id);
                    }
                }
                CommandId::SplitAtPlayhead => {
                    if hit_id >= 0 {
                        let frame = app_state::active_world(state)
                            .lock()
                            .unwrap()
                            .current_frame();
                        self.split_object_at(state, preview_panel, hit_id, frame);
                    }
                }
                CommandId::Duplicate => self.duplicate_requested(state, preview_panel, hit_id),
                CommandId::Cut => self.cut_requested(state, preview_panel, hit_id),
                CommandId::Copy => self.copy_requested(state, hit_id),
                CommandId::Paste => self.paste_requested(state, preview_panel),
                CommandId::ToggleRipple => self.ripple_mode = !self.ripple_mode,
                CommandId::ZoomIn => self.zoom_scale = (self.zoom_scale * 1.25).min(10.0),
                CommandId::ZoomOut => self.zoom_scale = (self.zoom_scale * 0.8).max(0.1),
                CommandId::SeekHome => self.seek(state, preview_panel, 0),
                CommandId::SeekEnd => {
                    let total = app_state::active_world(state)
                        .lock()
                        .unwrap()
                        .total_frames();
                    self.seek(state, preview_panel, total);
                }
                CommandId::Undo => {
                    if app_state::undo_active(state) {
                        self.after_structural_edit(state, preview_panel);
                    }
                }
                CommandId::Redo => {
                    if app_state::redo_active(state) {
                        self.after_structural_edit(state, preview_panel);
                    }
                }
                _ => {}
            }
        }
    }
}
