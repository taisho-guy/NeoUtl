use crate::app_state::SharedAppState;
use crate::gpu_shared::SharedGpu;
use crate::ui::launcher::LauncherPanel;
use crate::ui::preview::PreviewPanel;
use crate::ui::properties::PropertiesPanel;
use crate::ui::timeline::TimelineWindow;
use egui_system_fonts::{FontRegion, FontStyle, set_with_region};
use egui_wgpu::Renderer as EguiRenderer;
use egui_wgpu::wgpu;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const SURFACE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// 旧Slint版(`properties.slint`等)の配色(#0e0e12系パネル/#24242c境界線/#8aabffアクセント)
/// をegui::Visualsへ移植する。ウィンドウ生成のたびに一度だけ適用する。
pub struct RegisteredPreview {
    pub panel: Rc<RefCell<PreviewPanel>>,
    pub dialogs: Rc<RefCell<crate::ui::dialogs::DialogSet>>,
    pub timeline: Rc<RefCell<TimelineWindow>>,
    pub properties: Rc<RefCell<PropertiesPanel>>,
    pub state: SharedAppState,
}

pub type PreviewSlot = Rc<RefCell<Option<RegisteredPreview>>>;

pub fn make_preview_slot() -> PreviewSlot {
    Rc::new(RefCell::new(None))
}

pub fn set_preview(
    slot: &PreviewSlot,
    panel: Rc<RefCell<PreviewPanel>>,
    dialogs: Rc<RefCell<crate::ui::dialogs::DialogSet>>,
    timeline: Rc<RefCell<TimelineWindow>>,
    properties: Rc<RefCell<PropertiesPanel>>,
    state: SharedAppState,
) {
    *slot.borrow_mut() = Some(RegisteredPreview {
        panel,
        dialogs,
        timeline,
        properties,
        state,
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowKind {
    Launcher,
    Preview,
    Timeline,
    Properties,
    SystemSettings,
    ProjectSettings,
    SceneSettings,
    Keybindings,
    Export,
    EffectAdd,
    EasingEditor,
}

impl WindowKind {
    fn title(self) -> &'static str {
        match self {
            Self::Launcher => "NeoUtl - プロジェクト",
            Self::Preview => "NeoUtl",
            Self::Timeline => "NeoUtl - 拡張編集",
            Self::Properties => "NeoUtl - オブジェクト設定",
            Self::SystemSettings => "NeoUtl - システム設定",
            Self::ProjectSettings => "プロジェクト設定",
            Self::SceneSettings => "シーン設定",
            Self::Keybindings => "ショートカット設定",
            Self::Export => "メディアの書き出し",
            Self::EffectAdd => "エフェクト追加",
            Self::EasingEditor => "NeoUtl - イージング編集",
        }
    }

    fn size(self) -> (u32, u32) {
        match self {
            Self::Launcher => (640, 420),
            Self::Preview | Self::Timeline | Self::Properties => (720, 540),
            Self::SystemSettings => (720, 540),
            Self::ProjectSettings => (520, 360),
            Self::SceneSettings => (520, 700),
            Self::Keybindings => (720, 540),
            Self::Export => (620, 560),
            Self::EffectAdd => (420, 560),
            Self::EasingEditor => (560, 520),
        }
    }

    /// ウィンドウ生成と同時に破棄可能な設定系ダイアログ種別。
    /// 常駐ウィンドウ(Launcher/Preview/Timeline/Properties)とは異なり、
    /// 実体ウィンドウの有無そのものが開閉状態を表す（非表示のまま保持しない）。
    fn is_lazy_dialog(self) -> bool {
        matches!(
            self,
            Self::SystemSettings
                | Self::ProjectSettings
                | Self::SceneSettings
                | Self::Keybindings
                | Self::Export
                | Self::EffectAdd
                | Self::EasingEditor
        )
    }
}

/// SlintのWindow 1枚に対応するegui/winitの実ウィンドウ。
/// WindowごとにContext・入力状態・Renderer・Surfaceを分離し、擬似ウィンドウ合成はしない。
struct NativeWindow {
    kind: WindowKind,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: EguiRenderer,
}

impl NativeWindow {
    fn create(event_loop: &ActiveEventLoop, gpu: &SharedGpu, kind: WindowKind) -> Self {
        let (width, height) = kind.size();
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(kind.title())
                        .with_inner_size(winit::dpi::PhysicalSize::new(width, height)),
                )
                .expect("eguiウィンドウ生成失敗"),
        );
        let surface = gpu
            .instance
            .create_surface(window.clone())
            .expect("wgpu Surface生成失敗");
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        set_with_region(&ctx, FontRegion::Japanese, FontStyle::Sans);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = EguiRenderer::new(&gpu.device, SURFACE_FORMAT, Default::default());
        Self {
            kind,
            window,
            surface,
            config,
            ctx,
            state,
            renderer,
        }
    }

    fn redraw(&mut self, gpu: &SharedGpu, draw: impl FnOnce(&mut egui::Ui, &mut EguiRenderer)) {
        crate::theme::install(&self.ctx);
        let raw_input = self.state.take_egui_input(&self.window);
        let mut draw = Some(draw);
        let output = self.ctx.run_ui(raw_input, |ui| {
            if let Some(draw) = draw.take() {
                draw(ui, &mut self.renderer);
            }
        });
        self.state
            .handle_platform_output(&self.window, output.platform_output);

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => {
                self.surface.configure(&gpu.device, &self.config);
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        for (id, delta) in &output.textures_delta.set {
            self.renderer
                .update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui-window-encoder"),
            });
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: output.pixels_per_point,
        };
        self.renderer
            .update_buffers(&gpu.device, &gpu.queue, &mut encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui-window-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &primitives, &screen);
        }
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        gpu.queue.submit(Some(encoder.finish()));
        frame.present();
        if self.ctx.has_requested_repaint() {
            self.window.request_redraw();
        }
    }
}

pub struct EguiMainWindow {
    gpu: Rc<SharedGpu>,
    slot: PreviewSlot,
    launcher: LauncherPanel,
    windows: HashMap<WindowId, NativeWindow>,
    project_windows_created: bool,
}

impl EguiMainWindow {
    fn new(gpu: Rc<SharedGpu>, slot: PreviewSlot) -> Self {
        Self {
            gpu,
            slot,
            launcher: LauncherPanel::new(),
            windows: HashMap::new(),
            project_windows_created: false,
        }
    }

    fn add_window(&mut self, event_loop: &ActiveEventLoop, kind: WindowKind) {
        let native = NativeWindow::create(event_loop, &self.gpu, kind);
        self.windows.insert(native.window.id(), native);
    }

    fn ensure_project_windows(&mut self, event_loop: &ActiveEventLoop) {
        if self.slot.borrow().is_none() || self.project_windows_created {
            return;
        }
        for native in self.windows.values() {
            if native.kind == WindowKind::Launcher {
                native.window.set_visible(false);
            }
        }
        self.add_window(event_loop, WindowKind::Preview);
        self.add_window(event_loop, WindowKind::Timeline);
        self.add_window(event_loop, WindowKind::Properties);
        self.project_windows_created = true;
    }

    /// 設定系ダイアログの開閉フラグを読み取る。プロジェクト未読込時はNone。
    fn dialog_open_state(&self, kind: WindowKind) -> Option<bool> {
        let slot = self.slot.borrow();
        let p = slot.as_ref()?;
        Some(match kind {
            WindowKind::SystemSettings => p.dialogs.borrow().system_settings.open,
            WindowKind::ProjectSettings => p.dialogs.borrow().project_settings.open,
            WindowKind::SceneSettings => p.dialogs.borrow().scene_settings.open,
            WindowKind::Keybindings => p.dialogs.borrow().keybindings.open,
            WindowKind::Export => p.dialogs.borrow().export_dialog.open,
            WindowKind::EffectAdd => p.properties.borrow().effect_add.open,
            WindowKind::EasingEditor => crate::ui::properties::easing_editor::is_open(),
            _ => return None,
        })
    }

    /// 開閉フラグとウィンドウ実体の有無を一致させる。生成/破棄のみを行い、
    /// 「作ってから隠す」は行わない。開閉状態の唯一の実装箇所。
    fn sync_dialog_windows(&mut self, event_loop: &ActiveEventLoop) {
        for kind in [
            WindowKind::SystemSettings,
            WindowKind::ProjectSettings,
            WindowKind::SceneSettings,
            WindowKind::Keybindings,
            WindowKind::Export,
            WindowKind::EffectAdd,
            WindowKind::EasingEditor,
        ] {
            let Some(desired_open) = self.dialog_open_state(kind) else {
                continue;
            };
            let existing_id = self
                .windows
                .iter()
                .find(|(_, native)| native.kind == kind)
                .map(|(id, _)| *id);
            match (desired_open, existing_id) {
                (true, None) => self.add_window(event_loop, kind),
                (false, Some(id)) => {
                    self.windows.remove(&id);
                }
                _ => {}
            }
        }
    }

    /// ダイアログの開閉フラグを書き換える。ウィンドウ実体の生成/破棄は
    /// `sync_dialog_windows`が次回同期時に行う。
    fn set_dialog_open(slot: &PreviewSlot, kind: WindowKind, open: bool) {
        let slot_ref = slot.borrow();
        let Some(p) = slot_ref.as_ref() else {
            return;
        };
        match kind {
            WindowKind::SystemSettings => p.dialogs.borrow_mut().system_settings.open = open,
            WindowKind::ProjectSettings => p.dialogs.borrow_mut().project_settings.open = open,
            WindowKind::SceneSettings => p.dialogs.borrow_mut().scene_settings.open = open,
            WindowKind::Keybindings => p.dialogs.borrow_mut().keybindings.open = open,
            WindowKind::Export => p.dialogs.borrow_mut().export_dialog.open = open,
            WindowKind::EffectAdd => p.properties.borrow_mut().effect_add.open = open,
            WindowKind::EasingEditor => {
                if !open {
                    crate::ui::properties::easing_editor::close();
                }
            }
            _ => {}
        }
    }

    fn redraw(&mut self, id: WindowId) {
        let Some(mut native) = self.windows.remove(&id) else {
            return;
        };
        match native.kind {
            WindowKind::Launcher => {
                let launcher = &mut self.launcher;
                let gpu = self.gpu.clone();
                let slot = self.slot.clone();
                native.redraw(&self.gpu, |ui, _| {
                    if slot.borrow().is_none() {
                        if let Some(meta) = launcher.show(ui) {
                            crate::ui::start_project(meta, gpu, slot);
                        }
                    }
                });
            }
            WindowKind::Preview => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, renderer| {
                        p.panel.borrow_mut().show(ui, renderer, &p.state);
                    });
                }
            }
            WindowKind::Timeline => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.timeline
                            .borrow_mut()
                            .show(&ctx, ui, &p.state, &p.panel, &(), &p.dialogs);
                    });
                }
            }
            WindowKind::Properties => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.properties.borrow_mut().show(&ctx, ui, &p.state);
                    });
                }
            }
            WindowKind::SystemSettings => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let mut dialogs = p.dialogs.borrow_mut();
                        let ctx = ui.ctx().clone();
                        dialogs.system_settings.show(
                            &ctx,
                            ui,
                            &crate::app_state::settings_world(&p.state),
                        );
                    });
                }
            }
            WindowKind::ProjectSettings => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.dialogs
                            .borrow_mut()
                            .project_settings
                            .show(&ctx, ui, &p.state);
                    });
                }
            }
            WindowKind::SceneSettings => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.dialogs
                            .borrow_mut()
                            .scene_settings
                            .show(&ctx, ui, &p.state);
                    });
                }
            }
            WindowKind::Keybindings => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.dialogs.borrow_mut().keybindings.show(&ctx, ui)
                    });
                }
            }
            WindowKind::Export => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        p.dialogs
                            .borrow_mut()
                            .export_dialog
                            .show(&ctx, ui, &p.state)
                    });
                }
            }
            WindowKind::EffectAdd => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        p.properties
                            .borrow_mut()
                            .show_effect_add(ui.ctx(), &p.state);
                    });
                }
            }
            WindowKind::EasingEditor => {
                if let Some(p) = self.slot.borrow().as_ref() {
                    native.redraw(&self.gpu, |ui, _| {
                        let ctx = ui.ctx().clone();
                        let holder = crate::app_state::active_world(&p.state);
                        let mut world = holder.lock().unwrap();
                        if !crate::ui::properties::easing_editor::show(&ctx, ui, &mut world) {
                            crate::ui::properties::easing_editor::close();
                        }
                    });
                }
            }
        }
        self.windows.insert(id, native);
    }
}

impl ApplicationHandler for EguiMainWindow {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.windows.is_empty() {
            self.add_window(event_loop, WindowKind::Launcher);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = self.windows.get_mut(&id) else {
            return;
        };
        if native.state.on_window_event(&native.window, &event).repaint {
            native.window.request_redraw();
        }
        let kind = native.kind;
        match event {
            WindowEvent::CloseRequested => match kind {
                WindowKind::Launcher | WindowKind::Preview => event_loop.exit(),
                WindowKind::Timeline | WindowKind::Properties => native.window.set_visible(false),
                _ if kind.is_lazy_dialog() => Self::set_dialog_open(&self.slot, kind, false),
                _ => {}
            },
            WindowEvent::Resized(size) => {
                native.config.width = size.width.max(1);
                native.config.height = size.height.max(1);
                native.surface.configure(&self.gpu.device, &native.config);
            }
            WindowEvent::RedrawRequested => self.redraw(id),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.ensure_project_windows(event_loop);
        if let Some(p) = self.slot.borrow().as_ref() {
            p.dialogs
                .borrow_mut()
                .sync_preview_requests(&p.state, &p.panel);
        }
        self.sync_dialog_windows(event_loop);
        for native in self.windows.values() {
            if native.window.is_visible().unwrap_or(true) {
                native.window.request_redraw();
            }
        }
    }
}

pub fn run(gpu: Rc<SharedGpu>, slot: PreviewSlot) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = EguiMainWindow::new(gpu, slot);
    event_loop.run_app(&mut app)?;
    Ok(())
}
