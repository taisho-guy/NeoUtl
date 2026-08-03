use crate::app_state::{self, SharedAppState};
use crate::ecs::EcsWorld;
use crate::ecs::components::{MediaSource, ShapeParams, TextContent};
use crate::objects::registry;
use crate::shortcuts::{self, CommandId, Scope};
use crate::ui::dialogs::DialogSet;
use crate::ui::preview::PreviewPanel;
use crate::ui::types::{ContextMenuItem, LayerState, ObjectKindItem, SceneTabItem, TimelineObject};
use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Vec2};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

const HEADER_WIDTH: f32 = 60.0;
const LAYER_HEIGHT: f32 = 24.0;
const RULER_HEIGHT: f32 = 32.0;
const HANDLE_WIDTH: f32 = 6.0;
const KEYFRAME_SIZE: f32 = 8.0;

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

    fn reset_session(&mut self, world: &EcsWorld) {
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

    fn frame_interval(&self) -> i32 {
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

    fn frame_to_x(&self, frame: i32) -> f32 {
        frame as f32 * self.zoom_scale - self.scroll_x
    }

    fn px_to_frame(&self, px: f32) -> i32 {
        ((px + self.scroll_x) / self.zoom_scale).floor().max(0.0) as i32
    }

    fn layer_to_y(&self, layer: i32) -> f32 {
        layer as f32 * LAYER_HEIGHT - self.scroll_y
    }

    fn px_to_layer(&self, py: f32) -> i32 {
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

    fn body(
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
                    name: plugin.name.clone(),
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

    fn handle_shortcuts(
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

    fn scene_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        scene_tabs: &[SceneTabItem],
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        ui.horizontal(|ui| {
            for tab in scene_tabs {
                ui.horizontal(|ui| {
                    if ui.selectable_label(tab.active, &tab.name).clicked() {
                        self.switch_scene_tab(state, preview_panel, tab.id);
                    }
                    if ui.small_button("⚙").clicked() {
                        dialogs.borrow_mut().open_scene_edit(state, tab.id);
                    }
                    if ui.small_button("✕").clicked() {
                        self.close_scene_tab(state, preview_panel, tab.id);
                    }
                });
            }
            if ui.button("＋").clicked() {
                dialogs.borrow_mut().open_scene_create(state);
            }
        });
    }

    fn ruler(
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

    fn layer_header(
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
        ui.set_clip_rect(rect);

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

    #[allow(clippy::too_many_arguments)]
    fn timeline_view(
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
        ui.set_clip_rect(rect);

        let frame_interval = self.frame_interval();
        for i in 0..layer_count {
            let y = rect.min.y + self.layer_to_y(i);
            if y + LAYER_HEIGHT < rect.min.y || y > rect.max.y {
                continue;
            }
            if i % 2 == 0 {
                let row = Rect::from_min_size(
                    Pos2::new(rect.min.x, y),
                    Vec2::new(rect.width(), LAYER_HEIGHT),
                );
                painter.rect_filled(row, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 5));
            }
        }
        let line_count = if self.zoom_scale > 0.0 {
            (rect.width() / (self.zoom_scale * frame_interval.max(1) as f32)).ceil() as i32 + 1
        } else {
            0
        };
        let first_visible =
            (self.scroll_x / self.zoom_scale / frame_interval.max(1) as f32).floor() as i32;
        for i in 0..line_count {
            let frame = (first_visible + i) * frame_interval;
            let x = rect.min.x + self.frame_to_x(frame);
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            );
        }

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

    fn handle_background_input(
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

    #[allow(clippy::too_many_arguments)]
    fn clip_ui(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        view_rect: Rect,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        _props_weak: &(),
        obj: &TimelineObject,
        locked: bool,
        total_frames: i32,
    ) {
        let _ = total_frames;
        let (preview_start, preview_end, preview_layer) = match &self.drag {
            Some(d) if d.id == obj.id => (d.preview_start, d.preview_end, d.preview_layer),
            _ => (obj.start_frame, obj.end_frame, obj.layer),
        };

        let x = view_rect.min.x + self.frame_to_x(preview_start);
        let y = view_rect.min.y + self.layer_to_y(preview_layer) + LAYER_HEIGHT * 0.1;
        let w = ((preview_end - preview_start) as f32 * self.zoom_scale).max(4.0);
        let h = LAYER_HEIGHT * 0.8;
        let clip_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        if clip_rect.max.x < view_rect.min.x || clip_rect.min.x > view_rect.max.x {
            return;
        }
        if clip_rect.max.y < view_rect.min.y || clip_rect.min.y > view_rect.max.y {
            return;
        }

        let palette = [
            Color32::from_rgb(0x2a, 0x4d, 0xb8),
            Color32::from_rgb(0x25, 0x6e, 0x3c),
            Color32::from_rgb(0x6a, 0x2d, 0xb8),
            Color32::from_rgb(0xb8, 0x86, 0x2a),
            Color32::from_rgb(0x2a, 0xb8, 0xa8),
            Color32::from_rgb(0xb8, 0x2a, 0x5f),
        ];
        let base = if !obj.kind_known {
            Color32::from_rgb(0x5a, 0x5a, 0x5a)
        } else {
            palette[(obj.kind.max(0) as usize) % palette.len()]
        };
        let color = if obj.selected {
            brighten(base, 0.4)
        } else {
            base
        };
        painter.rect_filled(clip_rect, 3.0, color);
        painter.rect_stroke(
            clip_rect,
            3.0,
            Stroke::new(
                if obj.selected { 2.0 } else { 1.0 },
                if obj.selected {
                    Color32::WHITE
                } else {
                    darken(base, 0.3)
                },
            ),
            egui::StrokeKind::Outside,
        );

        if obj.has_waveform {
            if let Some(tex) = obj.waveform {
                let wx = clip_rect.min.x
                    + 3.0
                    + (obj.waveform_origin_frame - preview_start) as f32 * self.zoom_scale;
                let ww = (obj.waveform_duration_frames as f32 * self.zoom_scale).max(1.0);
                let wave_rect = Rect::from_min_size(
                    Pos2::new(wx, clip_rect.min.y + 3.0),
                    Vec2::new(ww, (clip_rect.height() - 6.0).max(1.0)),
                );
                let tint = if obj.selected {
                    Color32::from_white_alpha(230)
                } else {
                    Color32::from_white_alpha(166)
                };
                painter.image(
                    tex,
                    wave_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    tint,
                );
            }
        }

        painter.text(
            Pos2::new(clip_rect.min.x + 6.0, clip_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &obj.label,
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );

        for &f in &obj.keyframe_frames {
            let dragged_delta = match &self.kdrag {
                Some(k) if k.id == obj.id && k.frame == f => k.delta_frames,
                _ => 0,
            };
            let span = (obj.end_frame - obj.start_frame).max(1) as f32;
            let kx = clip_rect.min.x
                + (f + dragged_delta - obj.start_frame) as f32 * clip_rect.width() / span;
            let ky = clip_rect.center().y;
            let marker_rect = Rect::from_center_size(Pos2::new(kx, ky), Vec2::splat(KEYFRAME_SIZE));
            painter.rect_filled(marker_rect, 0.0, Color32::from_rgb(0xff, 0xd2, 0x3a));
            painter.rect_stroke(
                marker_rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(0x8a, 0x6a, 0x10)),
                egui::StrokeKind::Outside,
            );

            let kid = ui.id().with(("keyframe", obj.id, f));
            let kresp = ui.interact(marker_rect, kid, Sense::click_and_drag());
            if !locked {
                if kresp.drag_started() {
                    if let Some(pos) = kresp.interact_pointer_pos() {
                        self.kdrag = Some(KeyframeDrag {
                            id: obj.id,
                            frame: f,
                            press: pos,
                            delta_frames: 0,
                        });
                    }
                }
                if kresp.dragged() {
                    if let (Some(pos), Some(kdrag)) =
                        (kresp.interact_pointer_pos(), &mut self.kdrag)
                    {
                        kdrag.delta_frames =
                            ((pos.x - kdrag.press.x) / self.zoom_scale).floor() as i32;
                    }
                }
                if kresp.drag_stopped() {
                    if let Some(kdrag) = self.kdrag.take() {
                        if kdrag.delta_frames != 0 {
                            let new_frame =
                                (f + kdrag.delta_frames).clamp(obj.start_frame, obj.end_frame);
                            self.keyframe_moved(state, obj.id, f, new_frame);
                        } else if kresp.clicked() {
                        }
                    }
                }
            }
        }

        let body_rect = Rect::from_min_size(
            Pos2::new(clip_rect.min.x + HANDLE_WIDTH, clip_rect.min.y),
            Vec2::new(
                (clip_rect.width() - 2.0 * HANDLE_WIDTH).max(0.0),
                clip_rect.height(),
            ),
        );
        let left_rect =
            Rect::from_min_size(clip_rect.min, Vec2::new(HANDLE_WIDTH, clip_rect.height()));
        let right_rect = Rect::from_min_size(
            Pos2::new(clip_rect.max.x - HANDLE_WIDTH, clip_rect.min.y),
            Vec2::new(HANDLE_WIDTH, clip_rect.height()),
        );

        let left_id = ui.id().with(("clip-left", obj.id));
        let left_resp = ui.interact(left_rect, left_id, Sense::click_and_drag());
        let right_id = ui.id().with(("clip-right", obj.id));
        let right_resp = ui.interact(right_rect, right_id, Sense::click_and_drag());
        let body_id = ui.id().with(("clip-body", obj.id));
        let body_resp = ui.interact(body_rect, body_id, Sense::click_and_drag());

        if locked {
            return;
        }

        if left_resp.drag_started() {
            if let Some(pos) = left_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::ResizeLeft,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if right_resp.drag_started() {
            if let Some(pos) = right_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::ResizeRight,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if body_resp.drag_started() {
            self.select_object(state, &(), obj.id, false);
            if let Some(pos) = body_resp.interact_pointer_pos() {
                self.drag = Some(ClipDrag {
                    id: obj.id,
                    mode: DragMode::Move,
                    press: pos,
                    start_frame: obj.start_frame,
                    end_frame: obj.end_frame,
                    layer: obj.layer,
                    duration: obj.end_frame - obj.start_frame,
                    preview_start: obj.start_frame,
                    preview_end: obj.end_frame,
                    preview_layer: obj.layer,
                });
            }
        }
        if body_resp.clicked() && self.drag.is_none() {
            self.select_object(state, &(), obj.id, false);
        }
        if body_resp.secondary_clicked() {
            self.select_object(state, &(), obj.id, false);
            if let Some(pos) = body_resp.interact_pointer_pos() {
                self.open_context_menu(ui, state, pos, obj.start_frame, obj.layer, obj.id);
            }
        }

        if let Some(drag) = &mut self.drag {
            if drag.id == obj.id {
                match drag.mode {
                    DragMode::Move => {
                        if let Some(pos) = body_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            let dy = ((pos.y - drag.press.y) / LAYER_HEIGHT).floor() as i32;
                            drag.preview_start = (drag.start_frame + dx).max(0);
                            drag.preview_end = drag.preview_start + drag.duration;
                            drag.preview_layer = (drag.layer + dy).max(0);
                        }
                    }
                    DragMode::ResizeLeft => {
                        if let Some(pos) = left_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            drag.preview_start =
                                (drag.start_frame + dx).clamp(0, drag.end_frame - 1);
                        }
                    }
                    DragMode::ResizeRight => {
                        if let Some(pos) = right_resp
                            .interact_pointer_pos()
                            .or(ui.ctx().pointer_hover_pos())
                        {
                            let dx = ((pos.x - drag.press.x) / self.zoom_scale).floor() as i32;
                            drag.preview_end = (drag.end_frame + dx).max(drag.start_frame + 1);
                        }
                    }
                }
            }
        }
    }

    fn finish_drag_if_released(
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

    fn open_context_menu(
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

    fn context_menu_layer(
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

    fn apply_menu_action(
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

    fn after_structural_edit(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn seek(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        frame: i32,
    ) {
        preview_panel.borrow_mut().seek(frame, state);
    }

    fn select_object(
        &mut self,
        _state: &SharedAppState,
        _props_weak: &(),
        id: i32,
        additive: bool,
    ) {
        if !additive {
            self.selected_ids.clear();
        }
        self.selected_ids.insert(id);
    }

    fn selection_target_ids(&self, hit_id: i32) -> Vec<usize> {
        if self.selected_ids.len() > 1 && self.selected_ids.contains(&hit_id) {
            self.selected_ids.iter().map(|&id| id as usize).collect()
        } else {
            vec![hit_id as usize]
        }
    }

    fn add_object_at(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        frame: i32,
        layer: i32,
        kind_idx: i32,
    ) {
        let registry_snapshot = registry();
        let Some(plugin) = registry_snapshot.get(kind_idx as usize) else {
            return;
        };
        let start = frame.max(0);
        let layer = layer.max(0);
        let kind_id = kind_idx as u32;

        match plugin.name.as_str() {
            "Video" | "Image" | "Audio" => {
                let Some(path) = rfd::FileDialog::new().pick_file() else {
                    return;
                };
                let Some(kind) = crate::media::detect_kind(&path) else {
                    return;
                };
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                let media = MediaSource {
                    path,
                    kind,
                    trim_in_frame: 0,
                };
                world.add_media_object(start, 90, kind_id, layer, media);
            }
            "Text" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_object(start, 90, kind_id, layer, Some(TextContent::default()));
            }
            "Shape" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_shape_object(start, 90, kind_id, layer, ShapeParams::default());
            }
            "Scene" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                let host_scene = world.active_scene();
                let default_target = world
                    .scenes()
                    .into_iter()
                    .map(|s| s.id)
                    .find(|&id| !world.would_create_scene_cycle(host_scene, id));
                let Some(default_target) = default_target else {
                    eprintln!(
                        "[NeoUtl] シーンオブジェクト追加を中止: 配置可能なシーンがありません"
                    );
                    return;
                };
                world.add_scene_object(start, 90, kind_id, layer, default_target);
            }
            _ => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_object(start, 90, kind_id, layer, None);
            }
        }
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn delete_object(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
    ) {
        if id < 0 {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        world_holder.lock().unwrap().delete_object(id as usize);
        self.selected_ids.remove(&id);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn split_object_at(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
        frame: i32,
    ) {
        if id < 0 {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        world_holder
            .lock()
            .unwrap()
            .split_object(id as usize, frame);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn keyframe_moved(&mut self, state: &SharedAppState, id: i32, old_frame: i32, new_frame: i32) {
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        world_holder
            .lock()
            .unwrap()
            .move_keyframe(id as usize, "", old_frame, new_frame);
    }

    fn duplicate_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        hit_id: i32,
    ) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let frame = world.current_frame();
        world.duplicate_objects(&ids, frame, self.selected_layer);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn cut_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        hit_id: i32,
    ) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let docs = world.cut_objects(&ids);
        drop(world);
        app_state::set_clipboard(state, docs);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn copy_requested(&mut self, state: &SharedAppState, hit_id: i32) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let docs = world.copy_objects(&ids);
        drop(world);
        app_state::set_clipboard(state, docs);
    }

    fn paste_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        let docs = app_state::clipboard(state);
        if docs.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let frame = world.current_frame();
        world.paste_objects(&docs, frame, self.selected_layer);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn toggle_layer_visible(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        layer: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let visible = world
            .layer_states()
            .get(layer as usize)
            .map_or(true, |s| s.0);
        world.set_layer_visible(layer as usize, !visible);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn toggle_layer_locked(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        layer: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let locked = world
            .layer_states()
            .get(layer as usize)
            .map_or(false, |s| s.1);
        world.set_layer_locked(layer as usize, !locked);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    fn switch_scene_tab(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        if world.switch_scene(id) {
            drop(world);
            self.selected_ids.clear();
            preview_panel.borrow_mut().refresh_total_frames(state);
        }
    }

    fn close_scene_tab(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
    ) {
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        if world.scenes().len() > 1 {
            if world.remove_scene(id) {
                drop(world);
                preview_panel.borrow_mut().refresh_total_frames(state);
            } else {
                eprintln!("[NeoUtl] シーン削除を拒否: id={id}（他シーンのSceneObjectから参照中）");
            }
        }
    }

    fn to_egui(
        &mut self,
        ctx: &egui::Context,
        data: &crate::ecs::TimelineData,
        fps: f64,
    ) -> TimelineObject {
        let registry_snapshot = registry();
        let plugin = registry_snapshot.get(data.kind as usize);
        let waveform = data
            .media_path
            .as_deref()
            .and_then(|path| self.waveform_texture(ctx, path));
        let waveform_duration_frames = data
            .media_path
            .as_deref()
            .and_then(|path| {
                crate::media::cache::global()
                    .load_audio(path)
                    .ok()
                    .map(|audio| {
                        (audio.frame_count() as f64 / audio.sample_rate as f64 * fps).ceil() as i32
                    })
            })
            .unwrap_or(0);
        TimelineObject {
            id: data.id,
            start_frame: data.start_frame,
            end_frame: data.end_frame,
            kind: data.kind,
            kind_known: plugin.is_some(),
            layer: data.layer,
            label: plugin.map_or("Unknown", |p| p.name.as_str()).to_string(),
            selected: false,
            keyframe_frames: Vec::new(),
            waveform: waveform.map(|h| h.id()),
            has_waveform: waveform_duration_frames > 0,
            waveform_origin_frame: -data.media_trim_in_frame as i32,
            waveform_duration_frames,
        }
    }

    /// 波形テクスチャは音声デコード＋波形生成を伴うため、パス単位でegui::TextureHandleを
    /// 保持し続ける（毎フレーム再構築は行わない）。ハンドルを保持しない限りegui側で
    /// 参照カウントが尽き解放されるため、waveform_cacheが唯一の保持元となる。
    fn waveform_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let key = path.to_path_buf();
        if let Some(handle) = self.waveform_cache.get(&key) {
            return Some(handle.clone());
        }
        let audio = crate::media::cache::global().load_audio(path).ok()?;
        let asset = crate::media::waveform::get(path).unwrap_or_else(|| {
            let asset = crate::media::waveform::build(path, &audio);
            crate::media::waveform::insert(asset.clone());
            asset
        });
        let peaks = crate::media::waveform::level_for_columns(&asset, 512);
        let visible_peaks = peaks.as_ref();
        let width = 512usize;
        let height = 48usize;
        let mut pixels = vec![Color32::TRANSPARENT; width * height];
        for x in 0..width {
            let Some(peak) = visible_peaks.get(x * visible_peaks.len() / width) else {
                continue;
            };
            let center = height as i32 / 2;
            let top = ((1.0 - peak.max.clamp(-1.0, 1.0)) * center as f32).round() as i32;
            let bottom = ((1.0 - peak.min.clamp(-1.0, 1.0)) * center as f32).round() as i32;
            for y in top.max(0)..bottom.min(height as i32) {
                if let Some(px) = pixels.get_mut(y as usize * width + x) {
                    *px = Color32::from_rgba_unmultiplied(92, 177, 255, 210);
                }
            }
        }
        let image = egui::ColorImage {
            size: [width, height],
            source_size: egui::vec2(width as f32, height as f32),
            pixels,
        };
        let handle = ctx.load_texture(
            format!("waveform-{}", path.display()),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.waveform_cache.insert(key, handle.clone());
        Some(handle)
    }
}

fn brighten(c: Color32, factor: f32) -> Color32 {
    let f = |v: u8| {
        (v as f32 + (255.0 - v as f32) * factor)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

fn darken(c: Color32, factor: f32) -> Color32 {
    let f = |v: u8| (v as f32 * (1.0 - factor)).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// タイムライン右クリックメニューの項目集合を構築する唯一の経路。
/// hit-id>=0（クリップ上）: 削除→分割→複製→区切り→切り取り→コピー→区切り→リップルモード切替。
/// hit-id<0（背景上）: 登録済みオブジェクト種別ごとのAdd項目→区切り→元に戻す→やり直す→貼り付け。
fn build_context_menu(
    hit_id: i32,
    ripple_mode: bool,
    clipboard_empty: bool,
    kinds: &[ObjectKindItem],
) -> Vec<ContextMenuItem> {
    let sep = || ContextMenuItem {
        label: String::new(),
        action: 4,
        kind: -1,
        enabled: false,
        icon: String::new(),
    };
    if hit_id >= 0 {
        return vec![
            ContextMenuItem {
                label: "削除".into(),
                action: 1,
                kind: -1,
                enabled: true,
                icon: "trash".into(),
            },
            ContextMenuItem {
                label: "再生位置で分割".into(),
                action: 0,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
            },
            ContextMenuItem {
                label: "複製".into(),
                action: 7,
                kind: -1,
                enabled: true,
                icon: "copy-plus".into(),
            },
            sep(),
            ContextMenuItem {
                label: "切り取り".into(),
                action: 8,
                kind: -1,
                enabled: true,
                icon: "scissors".into(),
            },
            ContextMenuItem {
                label: "コピー".into(),
                action: 9,
                kind: -1,
                enabled: true,
                icon: "copy".into(),
            },
            sep(),
            ContextMenuItem {
                label: if ripple_mode {
                    "リップルモード: オン".into()
                } else {
                    "リップルモード: オフ".into()
                },
                action: 3,
                kind: -1,
                enabled: true,
                icon: "link".into(),
            },
        ];
    }
    let mut items: Vec<ContextMenuItem> = kinds
        .iter()
        .map(|k| ContextMenuItem {
            label: format!("{}を追加", k.name),
            action: 2,
            kind: k.kind,
            enabled: true,
            icon: "circle-plus".into(),
        })
        .collect();
    items.push(sep());
    items.push(ContextMenuItem {
        label: "元に戻す".into(),
        action: 5,
        kind: -1,
        enabled: true,
        icon: "undo".into(),
    });
    items.push(ContextMenuItem {
        label: "やり直す".into(),
        action: 6,
        kind: -1,
        enabled: true,
        icon: "redo".into(),
    });
    items.push(ContextMenuItem {
        label: "貼り付け".into(),
        action: 10,
        kind: -1,
        enabled: !clipboard_empty,
        icon: "paste".into(),
    });
    items
}

fn egui_key_name(key: egui::Key) -> String {
    use egui::Key;
    match key {
        Key::Space => "Space".into(),
        Key::ArrowRight => "Right".into(),
        Key::ArrowLeft => "Left".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::F2 => "F2".into(),
        Key::F3 => "F3".into(),
        Key::F4 => "F4".into(),
        Key::F9 => "F9".into(),
        Key::F10 => "F10".into(),
        Key::F11 => "F11".into(),
        Key::F12 => "F12".into(),
        Key::Tab => "Tab".into(),
        Key::PageDown => "PageDown".into(),
        Key::PageUp => "PageUp".into(),
        Key::Delete => "Delete".into(),
        Key::Equals => "=".into(),
        Key::Minus => "-".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}
