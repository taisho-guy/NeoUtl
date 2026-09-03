use crate::app_state::{self, SharedAppState};
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::get_active_objects_system;
use crate::renderer::RenderEngine;
use crate::shortcuts::{self, CommandId, Scope};
use crate::ui::dialogs::DialogSet;
use crate::ui::timeline::util::egui_key_name;
use egui_wgpu::Renderer as EguiRenderer;
use egui_wgpu::wgpu;
use elegance::{BrowserTab, BrowserTabs, BrowserTabsEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

pub struct LegacyWindows {}

type PlaybackAnchor = Option<(Instant, i32)>;

const PLAYBACK_TICK_MS: u64 = 16;
const SPEED_NORMAL_PERCENT: i32 = 100;

pub struct PreviewPanel {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    texture_id: Option<egui::TextureId>,
    texture_dims: (u32, u32),
    is_playing: bool,
    playback_anchor: PlaybackAnchor,
    speed_percent: i32,
    current_frame: i32,
    total_frames: i32,
    fps: i32,
    session_generation: u64,
    last_rendered_key: Option<(u64, u64, i32, u64)>,
    pub open_system_settings: bool,
    pub open_project_settings: bool,
    pub open_keybindings: bool,
    pub open_timeline: bool,
    pub open_export: bool,
    pub open_properties: bool,
}

impl PreviewPanel {
    pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>, _legacy: LegacyWindows) -> Self {
        Self {
            device,
            queue,
            texture_id: None,
            texture_dims: (0, 0),
            is_playing: false,
            playback_anchor: None,
            speed_percent: SPEED_NORMAL_PERCENT,
            current_frame: 0,
            total_frames: 0,
            fps: 30,
            session_generation: 0,
            last_rendered_key: None,
            open_system_settings: false,
            open_project_settings: false,
            open_keybindings: false,
            open_timeline: false,
            open_export: false,
            open_properties: false,
        }
    }

    pub fn session_generation(&self) -> u64 {
        self.session_generation
    }

    pub fn refresh_total_frames(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        self.total_frames = world_holder.lock().unwrap().total_frames();
    }

    pub fn seek(&mut self, frame: i32, state: &SharedAppState) {
        self.apply_frame(frame, state);
        if self.is_playing {
            self.playback_anchor = Some((Instant::now(), self.current_frame));
        }
    }

    fn sync_resolution_fps(&mut self, proj: &ProjectResource) {
        self.fps = proj.fps as i32;
    }

    fn apply_frame_with_speed(&mut self, frame: i32, speed_percent: i32, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let mixer_holder = app_state::active_audio_mixer(state);
        let mut world = world_holder.lock().unwrap();
        let previous = world.current_frame();
        let clamped = frame.clamp(0, world.total_frames());
        world.set_current_frame(clamped);

        let mut mixer = mixer_holder.lock().unwrap();
        if (clamped - previous).abs() > 1 {
            mixer.reset();
        }
        mixer.process_frame(&world, clamped, f64::from(speed_percent) / 100.0);
        drop(mixer);
        drop(world);

        self.current_frame = clamped;
    }

    fn apply_frame(&mut self, frame: i32, state: &SharedAppState) {
        self.apply_frame_with_speed(frame, SPEED_NORMAL_PERCENT, state);
    }

    fn frame_from_anchor(
        anchor_instant: Instant,
        anchor_frame: i32,
        fps: i32,
        speed_percent: i32,
    ) -> i32 {
        if fps <= 0 || speed_percent <= 0 {
            return anchor_frame;
        }
        let elapsed_secs = anchor_instant.elapsed().as_secs_f64();
        let advanced_frames = elapsed_secs * f64::from(fps) * (f64::from(speed_percent) / 100.0);
        anchor_frame + advanced_frames.floor() as i32
    }

    fn advance_playback(&mut self, state: &SharedAppState) {
        if !self.is_playing {
            return;
        }
        let Some((anchor_instant, anchor_frame)) = self.playback_anchor else {
            return;
        };
        let speed_percent = self
            .speed_percent
            .max(crate::config::PLAYBACK_SPEED_MIN_PERCENT);
        let next = Self::frame_from_anchor(anchor_instant, anchor_frame, self.fps, speed_percent);

        {
            let world_holder = app_state::active_world(state);
            let world = world_holder.lock().unwrap();
            app_state::active_audio_mixer(state)
                .lock()
                .unwrap()
                .process_frame(&world, self.current_frame, f64::from(speed_percent) / 100.0);
        }

        if next >= self.total_frames {
            self.is_playing = false;
            self.playback_anchor = None;
            self.apply_frame_with_speed(self.total_frames, speed_percent, state);
            app_state::active_audio_mixer(state).lock().unwrap().pause();
        } else if next != self.current_frame {
            self.apply_frame_with_speed(next, speed_percent, state);
        }
    }

    fn render_frame(&mut self, egui_renderer: &mut EguiRenderer, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let engine_holder = app_state::active_engine(state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();

        let mut engine_lock = engine_holder.lock().unwrap();
        if engine_lock.is_none() {
            *engine_lock = Some(RenderEngine::new(
                self.device.as_ref().clone(),
                self.queue.as_ref().clone(),
                proj.width,
                proj.height,
            ));
        }
        let Some(engine) = engine_lock.as_mut() else {
            return;
        };

        if engine.render_width != proj.width || engine.render_height != proj.height {
            engine.resize_render_target(proj.width, proj.height);
            if let Some(old_id) = self.texture_id.take() {
                egui_renderer.free_texture(&old_id);
            }
        }

        let revision = world.revision();
        let media_generation = neoutl_media_runtime::cache::global().ready_generation();
        let key = (
            self.session_generation,
            revision,
            self.current_frame,
            media_generation,
        );
        let needs_recompose = self.texture_id.is_none() || self.last_rendered_key != Some(key);
        if needs_recompose {
            let (active, captured) = get_active_objects_system(&world);
            engine.render(&world, &active, &captured, &proj);
            self.last_rendered_key = Some(key);
        }

        if self.texture_id.is_none() {
            let view = engine
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(
                &self.device,
                &view,
                wgpu::FilterMode::Linear,
            );
            self.texture_id = Some(id);
            self.texture_dims = (proj.width, proj.height);
        }

        drop(world);
        self.sync_resolution_fps(&proj);
        self.total_frames = engine_lock.as_ref().map_or(0, |_| self.total_frames);
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, state: &SharedAppState) {
        use elegance::{MenuBar, MenuItem};
        let mut open_export = false;
        let mut open_system_settings = false;
        let mut open_project_settings = false;
        let mut open_keybindings = false;
        let mut open_timeline = false;
        let mut open_properties = false;
        ui.horizontal(|ui| {
            MenuBar::new("preview_menu_bar").show(ui, |bar| {
                bar.menu(t!("ファイル"), |ui| {
                    if ui.add(MenuItem::new(t!("新規プロジェクト"))).clicked() {
                        let _ = app_state::new_project_session(state);
                    }
                    if ui.add(MenuItem::new(t!("プロジェクトを開く"))).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let _ = app_state::open_project_session(state, &dir);
                        }
                    }
                    if ui.add(MenuItem::new(t!("上書き保存"))).clicked() {
                        let _ = app_state::save_active(state);
                    }
                    if ui.add(MenuItem::new(t!("名前を付けて保存"))).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let world_holder = app_state::active_world(state);
                            let doc = world_holder.lock().unwrap().to_document();
                            let _ = crate::project::save_document(&dir, &doc);
                        }
                    }
                    if ui.add(MenuItem::new(t!("メディアの書き出し"))).clicked() {
                        open_export = true;
                    }
                    ui.separator();
                    if ui.add(MenuItem::new(t!("終了"))).clicked() {
                        app_state::save_all(state);
                        std::process::exit(0);
                    }
                });
                bar.menu(t!("編集"), |ui| {
                    if ui.add(MenuItem::new(t!("元に戻す"))).clicked() {
                        app_state::undo_active(state);
                    }
                    if ui.add(MenuItem::new(t!("やり直し"))).clicked() {
                        app_state::redo_active(state);
                    }
                    ui.separator();
                    if ui.add(MenuItem::new(t!("システム設定"))).clicked() {
                        open_system_settings = true;
                    }
                    if ui.add(MenuItem::new(t!("プロジェクト設定"))).clicked() {
                        open_project_settings = true;
                    }
                    if ui.add(MenuItem::new(t!("ショートカット設定"))).clicked() {
                        open_keybindings = true;
                    }
                });
                bar.menu(t!("表示"), |ui| {
                    if ui.add(MenuItem::new(t!("拡張編集"))).clicked() {
                        open_timeline = true;
                    }
                    if ui.add(MenuItem::new(t!("プロパティ"))).clicked() {
                        open_properties = true;
                    }
                });
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.speed_percent)
                        .range(
                            crate::config::PLAYBACK_SPEED_MIN_PERCENT
                                ..=crate::config::PLAYBACK_SPEED_MAX_PERCENT,
                        )
                        .suffix("%"),
                );
                ui.label(t!("速度"));

                if ui
                    .add_sized([28.0, 28.0], elegance::Button::new("⏭"))
                    .clicked()
                {
                    self.apply_frame(self.current_frame + 1, state);
                }
                let icon = if self.is_playing { "⏸" } else { "▶" };
                if ui
                    .add_sized([28.0, 28.0], elegance::Button::new(icon))
                    .clicked()
                {
                    self.is_playing = !self.is_playing;
                    let mixer = app_state::active_audio_mixer(state);
                    if self.is_playing {
                        self.playback_anchor = Some((Instant::now(), self.current_frame));
                        mixer.lock().unwrap().play();
                    } else {
                        self.playback_anchor = None;
                        mixer.lock().unwrap().pause();
                    }
                }
                if ui
                    .add_sized([28.0, 28.0], elegance::Button::new("⏮"))
                    .clicked()
                {
                    self.apply_frame(self.current_frame - 1, state);
                }

                let digits = self.total_frames.max(1).to_string().len();
                ui.monospace(format!(
                    "{:0width$} / {}",
                    self.current_frame,
                    self.total_frames,
                    width = digits
                ));
            });
        });
        if open_export {
            self.open_export = true;
        }
        if open_system_settings {
            self.open_system_settings = true;
        }
        if open_project_settings {
            self.open_project_settings = true;
        }
        if open_keybindings {
            self.open_keybindings = true;
        }
        if open_timeline {
            self.open_timeline = true;
        }
        if open_properties {
            self.open_properties = true;
        }
    }

    fn tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        let (names, active_index) = {
            let s = state.lock().unwrap();
            let names: Vec<String> = s
                .sessions
                .iter()
                .map(|sess| sess.meta.name.clone())
                .collect();
            (names, s.active)
        };
        let closable = names.len() > 1;

        let mut tabs = BrowserTabs::new("session-tabs").show_new_button(false);
        for (index, name) in names.iter().enumerate() {
            tabs = tabs.with_tab(BrowserTab::new(index.to_string(), name.clone()));
        }
        tabs.set_selected(active_index.to_string());

        tabs.show(ui);

        let mut switch_target: Option<usize> = None;
        let mut close_target: Option<usize> = None;
        for event in tabs.take_events() {
            match event {
                BrowserTabsEvent::Activated(id) => switch_target = id.parse().ok(),
                BrowserTabsEvent::Closed(id) => {
                    if closable {
                        close_target = id.parse().ok();
                    }
                }
                BrowserTabsEvent::NewRequested => {}
            }
        }

        if let Some(i) = switch_target {
            {
                let mut s = state.lock().unwrap();
                s.active = i;
            }
            self.playback_anchor = None;
            self.texture_id = None;
            self.sync_active_session(state);
        }
        if let Some(i) = close_target {
            dialogs.borrow_mut().request_close_session(state, i);
        }
    }

    fn switch_relative(&mut self, state: &SharedAppState, delta: i32) {
        {
            let mut s = state.lock().unwrap();
            let len = s.sessions.len() as i32;
            if len <= 1 {
                return;
            }
            s.active = (s.active as i32 + delta).rem_euclid(len) as usize;
        }
        self.playback_anchor = None;
        self.texture_id = None;
        self.sync_active_session(state);
    }

    fn handle_project_shortcuts(
        &mut self,
        ui: &egui::Ui,
        state: &SharedAppState,
        dialogs: &Rc<RefCell<DialogSet>>,
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
            let Some(cmd) = shortcuts::resolve_active(Scope::Global, ctrl, shift, alt, &key) else {
                continue;
            };
            match cmd {
                CommandId::NextProjectTab => self.switch_relative(state, 1),
                CommandId::PrevProjectTab => self.switch_relative(state, -1),
                CommandId::CloseProjectTab => {
                    let active = state.lock().unwrap().active;
                    dialogs.borrow_mut().request_close_session(state, active);
                }
                _ => {}
            }
        }
    }

    fn confirm_close_dialog(
        &mut self,
        ctx: &egui::Context,
        state: &SharedAppState,
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        let Some(index) = dialogs.borrow().confirm_close_session else {
            return;
        };
        let mut save_and_close = false;
        let mut discard_and_close = false;
        let mut cancel = false;
        let mut modal_open = true;
        elegance::Modal::new("confirm_close_session", &mut modal_open)
            .heading(t!("未保存の変更"))
            .show(ctx, |ui| {
                ui.label(t!(
                    "保存されていない変更があります。閉じる前に保存しますか？"
                ));
                ui.horizontal(|ui| {
                    if ui
                        .add(elegance::Button::new(t!("保存して閉じる")))
                        .clicked()
                    {
                        save_and_close = true;
                    }
                    if ui
                        .add(elegance::Button::new(t!("保存せず閉じる")).outline())
                        .clicked()
                    {
                        discard_and_close = true;
                    }
                    if ui
                        .add(elegance::Button::new(t!("キャンセル")).outline())
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if !modal_open {
            cancel = true;
        }
        if save_and_close {
            app_state::save_session(state, index);
            let _ = app_state::close_session(state, index);
        } else if discard_and_close {
            let _ = app_state::close_session(state, index);
        } else if !cancel {
            return;
        }
        dialogs.borrow_mut().confirm_close_session = None;
        if save_and_close || discard_and_close {
            self.playback_anchor = None;
            self.texture_id = None;
            self.sync_active_session(state);
        }
    }

    fn playback_controls(&mut self, ui: &mut egui::Ui, state: &SharedAppState) {
        let mut frame = self.current_frame;
        let slider = ui.add_sized(
            [ui.available_width(), 20.0],
            elegance::Slider::new(&mut frame, 0..=self.total_frames.max(1)).show_value(false),
        );
        if slider.changed() {
            self.apply_frame(frame, state);
            if self.is_playing {
                self.playback_anchor = Some((Instant::now(), self.current_frame));
            }
        }
    }

    pub fn sync_active_session(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        self.total_frames = world.total_frames();
        drop(world);
        app_state::active_audio_mixer(state).lock().unwrap().pause();
        self.current_frame = 0;
        self.is_playing = false;
        self.sync_resolution_fps(&proj);
        self.session_generation += 1;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        egui_renderer: &mut EguiRenderer,
        state: &SharedAppState,
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        self.handle_project_shortcuts(ui, state, dialogs);
        self.menu_bar(ui, state);
        self.tab_bar(ui, state, dialogs);
        self.confirm_close_dialog(&ui.ctx().clone(), state, dialogs);

        self.render_frame(egui_renderer, state);

        egui::Panel::bottom("preview_playback_bar")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(6, 4)))
            .show(ui, |ui| {
                self.playback_controls(ui, state);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                if let Some(texture_id) = self.texture_id {
                    let (w, h) = self.texture_dims;
                    let aspect = w as f32 / h.max(1) as f32;
                    let avail = ui.available_size();
                    let size = if avail.x / avail.y > aspect {
                        egui::vec2(avail.y * aspect, avail.y)
                    } else {
                        egui::vec2(avail.x, avail.x / aspect)
                    };
                    ui.centered_and_justified(|ui| {
                        ui.add(egui::Image::new((texture_id, size)).fit_to_exact_size(size));
                    });
                }
            });

        if self.is_playing {
            self.advance_playback(state);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(PLAYBACK_TICK_MS));
        }
    }
}
