use crate::app_state::SharedAppState;
use crate::gpu_shared::SharedGpu;
use crate::ui::launcher::LauncherPanel;
use crate::ui::preview::PreviewPanel;
use crate::ui::properties::PropertiesPanel;
use crate::ui::timeline::TimelineWindow;
use egui_system_fonts;
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
    Splash,
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
            Self::Splash => "NeoUtl",
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
            Self::Splash => (0, 0),
            Self::Launcher => (640, 420),
            Self::Preview | Self::Timeline | Self::Properties => (720, 540),
            Self::SystemSettings => (720, 540),
            Self::ProjectSettings => (520, 360),
            Self::SceneSettings => (520, 700),
            Self::Keybindings => (720, 540),
            Self::Export => (620, 560),
            Self::EffectAdd => (420, 560),
            Self::EasingEditor => (580, 460),
        }
    }

    fn min_size(self) -> Option<(u32, u32)> {
        match self {
            Self::EffectAdd => Some((400, 240)),
            Self::SceneSettings => Some((600, 240)),
            Self::SystemSettings => Some((680, 240)),
            _ => None,
        }
    }

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

struct NativeWindow {
    kind: WindowKind,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: EguiRenderer,
    visible: bool,
}

impl NativeWindow {
    fn create(event_loop: &ActiveEventLoop, gpu: &SharedGpu, kind: WindowKind) -> Self {
        let (width, height) = kind.size();
        Self::create_sized(event_loop, gpu, kind, width, height)
    }

    fn create_sized(
        event_loop: &ActiveEventLoop,
        gpu: &SharedGpu,
        kind: WindowKind,
        width: u32,
        height: u32,
    ) -> Self {
        let mut attrs = Window::default_attributes()
            .with_title(kind.title())
            .with_inner_size(winit::dpi::LogicalSize::new(width as f64, height as f64));
        if kind == WindowKind::Splash {
            attrs = attrs
                .with_decorations(false)
                .with_resizable(false)
                .with_transparent(true);
        }
        if let Some((min_w, min_h)) = kind.min_size() {
            attrs =
                attrs.with_min_inner_size(winit::dpi::LogicalSize::new(min_w as f64, min_h as f64));
        }

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("eguiウィンドウ生成失敗"),
        );

        let surface = gpu
            .instance
            .create_surface(window.clone())
            .expect("wgpu Surface生成失敗");
        let caps = surface.get_capabilities(&gpu.adapter);
        let alpha_mode = if kind == WindowKind::Splash {
            if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                wgpu::CompositeAlphaMode::Auto
            }
        } else {
            wgpu::CompositeAlphaMode::Auto
        };
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: SURFACE_FORMAT,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gpu.device, &config);

        let ctx = egui::Context::default();
        egui_material_icons::initialize(&ctx);
        egui_extras::install_image_loaders(&ctx);
        crate::theme::install(&ctx);
        install_locale_fonts(&ctx);
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
            visible: true,
        }
    }

    fn redraw(&mut self, gpu: &SharedGpu, draw: impl FnOnce(&mut egui::Ui, &mut EguiRenderer)) {
        if !self.visible {
            return;
        }
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
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
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
        let clear_color = if self.kind == WindowKind::Splash {
            wgpu::Color::TRANSPARENT
        } else {
            let bg = self.ctx.style_of(self.ctx.theme()).visuals.panel_fill;
            wgpu::Color {
                r: bg.r() as f64 / 255.0,
                g: bg.g() as f64 / 255.0,
                b: bg.b() as f64 / 255.0,
                a: 1.0,
            }
        };
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui-window-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear_color),
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
        gpu.queue.present(frame);
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
    init_rx: std::sync::mpsc::Receiver<()>,
    init_done: bool,
}

impl EguiMainWindow {
    fn new(gpu: Rc<SharedGpu>, slot: PreviewSlot, init_rx: std::sync::mpsc::Receiver<()>) -> Self {
        Self {
            gpu,
            slot,
            launcher: LauncherPanel::new(),
            windows: HashMap::new(),
            project_windows_created: false,
            init_rx,
            init_done: false,
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
        let launcher_ids: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, native)| native.kind == WindowKind::Launcher)
            .map(|(id, _)| *id)
            .collect();
        for id in launcher_ids {
            self.windows.remove(&id);
        }
        self.add_window(event_loop, WindowKind::Preview);
        self.add_window(event_loop, WindowKind::Timeline);
        self.add_window(event_loop, WindowKind::Properties);
        self.project_windows_created = true;
    }

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
            WindowKind::Splash => {
                native.redraw(&self.gpu, |ui, _| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.add(egui::Image::new(crate::splash::SOURCE.clone()));
                            });
                        });
                });
            }
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
                    native
                        .window
                        .set_title(&crate::app_state::active_project_window_title(&p.state));
                    native.redraw(&self.gpu, |ui, renderer| {
                        p.panel
                            .borrow_mut()
                            .show(ui, renderer, &p.state, &p.dialogs);
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
                        p.properties.borrow_mut().show(&ctx, ui, &p.state, &p.panel);
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
                        p.properties.borrow_mut().show_effect_add(ui, &p.state);
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
            let (w, h) = crate::splash::WINDOW_SIZE;
            let native =
                NativeWindow::create_sized(event_loop, &self.gpu, WindowKind::Splash, w, h);
            self.windows.insert(native.window.id(), native);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = self.windows.get_mut(&id) else {
            return;
        };
        if native.visible && native.state.on_window_event(&native.window, &event).repaint {
            native.window.request_redraw();
        }
        let kind = native.kind;
        match event {
            WindowEvent::CloseRequested => match kind {
                WindowKind::Splash | WindowKind::Launcher | WindowKind::Preview => {
                    event_loop.exit()
                }
                WindowKind::Timeline | WindowKind::Properties => {
                    native.visible = false;
                    native.window.set_visible(false);
                }
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
        if !self.init_done {
            match self.init_rx.try_recv() {
                Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.init_done = true;
                    let splash_ids: Vec<WindowId> = self
                        .windows
                        .iter()
                        .filter(|(_, native)| native.kind == WindowKind::Splash)
                        .map(|(id, _)| *id)
                        .collect();
                    for id in splash_ids {
                        self.windows.remove(&id);
                    }
                    self.add_window(event_loop, WindowKind::Launcher);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
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

pub fn run(
    gpu: Rc<SharedGpu>,
    slot: PreviewSlot,
    init_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = EguiMainWindow::new(gpu, slot, init_rx);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn install_locale_fonts(ctx: &egui::Context) {
    egui_system_fonts::set_auto(ctx, egui_system_fonts::FontStyle::Sans);
}
