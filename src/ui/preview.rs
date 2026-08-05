use crate::app_state::{self, SharedAppState};
use crate::ecs::resources::ProjectResource;
use crate::ecs::systems::get_active_objects_system;
use crate::renderer::RenderEngine;
use crate::shortcuts::{self, CommandId, Scope};
use crate::ui::dialogs::DialogSet;
use crate::ui::timeline::util::egui_key_name;
use egui_wgpu::Renderer as EguiRenderer;
use egui_wgpu::wgpu;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

/// フェーズ5未移植の既存Slintウィンドウ。本体ウィンドウ・拡張編集はeguiへ移行済みのため、
/// メニューからの表示要求はこのWeakハンドル経由で行う。system_settings/project_settings/
/// keybindings/scene_settings/export_dialogはフェーズ2、拡張編集はフェーズ4でegui-native
/// 化済みのためここには含まれない（DialogSet・TimelineWindowがPreviewPanelの開閉要求
/// フラグを読む）。
pub struct LegacyWindows {}

/// 再生開始時刻と開始フレームの記録。current_frameはこの2値と経過実時間から
/// 決定論的に算出する（redraw頻度に非依存）。
type PlaybackAnchor = Option<(Instant, i32)>;

const PLAYBACK_TICK_MS: u64 = 16;
const SPEED_NORMAL_PERCENT: i32 = 100;

/// 本体ウィンドウの状態。deviceとqueueはinit_shared_gpuから起動時に確定した値を
/// 保持し続け、RenderEngineとegui_wgpu::Rendererの双方へ同一ハンドルを供給する。
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
            open_system_settings: false,
            open_project_settings: false,
            open_keybindings: false,
            open_timeline: false,
            open_export: false,
            open_properties: false,
        }
    }

    /// TimelineWindowがプル型再同期の要否を判定するための単調増加世代値。
    /// アクティブセッション切替（新規プロジェクト確定・タブ切替）の唯一の発生源である
    /// sync_active_session内でのみ加算する。
    pub fn session_generation(&self) -> u64 {
        self.session_generation
    }

    /// タイムライン側での構造編集（追加・削除・分割・複製・移動・貼り付け等）確定後、
    /// 総フレーム数のみをworldから再取得する。オブジェクト一覧自体はTimelineWindowが
    /// 毎フレームworldから直接読み出すため、ここでは同期しない。
    pub fn refresh_total_frames(&mut self, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        self.total_frames = world_holder.lock().unwrap().total_frames();
    }

    /// タイムライン・ルーラーからのシーク要求の唯一の受け口。
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

    /// GPU側描画とテクスチャ登録。ゼロコピー経路の中核。
    /// register_native_textureはリサイズ発生時のみ呼び、毎フレーム再登録しない。
    fn render_frame(&mut self, egui_renderer: &mut EguiRenderer, state: &SharedAppState) {
        let world_holder = app_state::active_world(state);
        let engine_holder = app_state::active_engine(state);
        let world = world_holder.lock().unwrap();
        let proj = world.get_project();
        let active = get_active_objects_system(&world);

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

        engine.render(&world, &active, &proj);

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
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t!("ファイル"), |ui| {
                if ui.button(t!("新規プロジェクト")).clicked() {
                    let _ = app_state::new_project_session(state);
                    ui.close();
                }
                if ui.button(t!("プロジェクトを開く")).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let _ = app_state::open_project_session(state, &dir);
                    }
                    ui.close();
                }
                if ui.button(t!("上書き保存")).clicked() {
                    let _ = app_state::save_active(state);
                    ui.close();
                }
                if ui.button(t!("名前を付けて保存")).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let world_holder = app_state::active_world(state);
                        let doc = world_holder.lock().unwrap().to_document();
                        let _ = crate::project::save_document(&dir, &doc);
                    }
                    ui.close();
                }
                if ui.button(t!("メディアの書き出し")).clicked() {
                    self.open_export = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(t!("終了")).clicked() {
                    app_state::save_all(state);
                    std::process::exit(0);
                }
            });
            ui.menu_button(t!("編集"), |ui| {
                if ui.button(t!("元に戻す")).clicked() {
                    app_state::undo_active(state);
                    ui.close();
                }
                if ui.button(t!("やり直し")).clicked() {
                    app_state::redo_active(state);
                    ui.close();
                }
                ui.separator();
                if ui.button(t!("システム設定")).clicked() {
                    self.open_system_settings = true;
                    ui.close();
                }
                if ui.button(t!("プロジェクト設定")).clicked() {
                    self.open_project_settings = true;
                    ui.close();
                }
                if ui.button(t!("ショートカット設定")).clicked() {
                    self.open_keybindings = true;
                    ui.close();
                }
            });
            ui.menu_button(t!("表示"), |ui| {
                if ui.button(t!("拡張編集")).clicked() {
                    self.open_timeline = true;
                    ui.close();
                }
                if ui.button(t!("プロパティ")).clicked() {
                    self.open_properties = true;
                    ui.close();
                }
            });
        });
    }

    /// QML `RowLayout{ readonly property int _tabH: 28 }` 対応。
    /// 各タブに閉じるボタン(×)を併設する。closable判定はセッション数>1のみ。
    fn tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        state: &SharedAppState,
        dialogs: &Rc<RefCell<DialogSet>>,
    ) {
        ui.set_min_height(28.0);
        ui.horizontal(|ui| {
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
            let mut switch_target: Option<usize> = None;
            let mut close_target: Option<usize> = None;
            for (i, name) in names.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.selectable_label(i == active_index, name).clicked() && i != active_index {
                        switch_target = Some(i);
                    }
                    if closable && ui.small_button("×").clicked() {
                        close_target = Some(i);
                    }
                });
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
        });
    }

    /// active_indexをdelta分循環移動する（Ctrl+Tab/Ctrl+Shift+Tab用）。
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

    /// プロジェクトタブ切替・クローズのグローバルショートカット処理。
    /// メニュー/タブバーとは独立し、フォーカス位置に関わらず常時解決する。
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

    /// dialogs.confirm_close_sessionが立っている間、保存確認モーダルを表示する。
    /// 「保存して閉じる」「保存せず閉じる」「キャンセル」の3択。
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
        egui::Window::new(t!("未保存の変更"))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(t!(
                    "保存されていない変更があります。閉じる前に保存しますか？"
                ));
                ui.horizontal(|ui| {
                    if ui.button(t!("保存して閉じる")).clicked() {
                        save_and_close = true;
                    }
                    if ui.button(t!("保存せず閉じる")).clicked() {
                        discard_and_close = true;
                    }
                    if ui.button(t!("キャンセル")).clicked() {
                        cancel = true;
                    }
                });
            });
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

    /// QML `MainWindow.qml` 再生コントロールバー(Rectangle height:38, RowLayout)の直接対応。
    /// 並び順: シークバー(fillWidth) → フレームカウンタ → 前後/再生ボタン群 → 速度SpinBox。
    fn playback_controls(&mut self, ui: &mut egui::Ui, state: &SharedAppState) {
        ui.set_min_height(38.0);
        ui.horizontal(|ui| {
            let mut frame = self.current_frame;
            let slider = ui.add_sized(
                [ui.available_width() - 220.0, 20.0],
                egui::Slider::new(&mut frame, 0..=self.total_frames.max(1)).show_value(false),
            );
            if slider.changed() {
                self.apply_frame(frame, state);
                if self.is_playing {
                    self.playback_anchor = Some((Instant::now(), self.current_frame));
                }
            }

            let digits = self.total_frames.max(1).to_string().len();
            ui.monospace(format!(
                "{:0width$} / {}",
                self.current_frame,
                self.total_frames,
                width = digits
            ));

            if ui.add_sized([32.0, 32.0], egui::Button::new("⏮")).clicked() {
                self.apply_frame(self.current_frame - 1, state);
            }
            let icon = if self.is_playing { "⏸" } else { "▶" };
            if ui
                .add_sized([32.0, 32.0], egui::Button::new(icon))
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
            if ui.add_sized([32.0, 32.0], egui::Button::new("⏭")).clicked() {
                self.apply_frame(self.current_frame + 1, state);
            }

            ui.label(t!("速度"));
            let mut speed = self.speed_percent;
            if ui
                .add_sized(
                    [80.0, 28.0],
                    egui::DragValue::new(&mut speed)
                        .range(
                            crate::config::PLAYBACK_SPEED_MIN_PERCENT
                                ..=crate::config::PLAYBACK_SPEED_MAX_PERCENT,
                        )
                        .custom_formatter(|v, _| format!("{:.1}x", v / 100.0))
                        .speed(1.0),
                )
                .changed()
            {
                self.speed_percent = speed;
                if self.is_playing {
                    self.playback_anchor = Some((Instant::now(), self.current_frame));
                }
            }
        });
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

    /// 毎フレーム呼び出しの単一窓口。呼び出し順序:
    /// メニュー/タブ/操作バー構築 → GPU描画・テクスチャ登録 → 中央パネルへ表示。
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

        const PLAYBACK_BAR_HEIGHT: f32 = 38.0;
        let total_height = ui.available_height();
        let image_height = (total_height - PLAYBACK_BAR_HEIGHT).max(0.0);

        ui.allocate_ui(egui::vec2(ui.available_width(), image_height), |ui| {
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

        ui.allocate_ui(
            egui::vec2(ui.available_width(), PLAYBACK_BAR_HEIGHT),
            |ui| {
                self.playback_controls(ui, state);
            },
        );

        if self.is_playing {
            self.advance_playback(state);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(PLAYBACK_TICK_MS));
        }
    }
}
