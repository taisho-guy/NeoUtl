pub mod shortcuts;
pub mod state;
pub mod theme;
pub mod update;

use crate::app::state::SharedAppState;
use crate::infra::gpu_shared::SharedGpu;
use crate::project::ProjectMeta;
use crate::ui::dialogs::DialogSet;
use crate::ui::launcher::LauncherPanel;
use crate::ui::preview::PreviewPanel;
use crate::ui::properties::PropertiesPanel;
use crate::ui::timeline::TimelineWindow;
use std::cell::RefCell;
use std::rc::Rc;

pub struct RegisteredPreview {
    pub panel: Rc<RefCell<PreviewPanel>>,
    pub dialogs: Rc<RefCell<DialogSet>>,
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
    dialogs: Rc<RefCell<DialogSet>>,
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

const DIALOG_WINDOW_IDS: &[&str] = &[
    "system_settings",
    "project_settings",
    "scene_settings",
    "keybindings",
    "export",
    "effect_add",
    "easing_editor",
];

const LAUNCHER_WINDOW_SIZE: (f32, f32) = (960.0, 640.0);

fn dialog_title(id: &str) -> String {
    match id {
        "system_settings" => t!("システム設定"),
        "project_settings" => t!("プロジェクト設定"),
        "scene_settings" => t!("シーン設定"),
        "keybindings" => t!("ショートカット設定"),
        "export" => t!("メディアの書き出し"),
        "effect_add" => t!("エフェクト追加"),
        "easing_editor" => t!("イージングエディタ"),
        _ => id.to_owned(),
    }
}

fn dialog_open_state(p: &RegisteredPreview, id: &str) -> bool {
    match id {
        "system_settings" => p.dialogs.borrow().system_settings.open,
        "project_settings" => p.dialogs.borrow().project_settings.open,
        "scene_settings" => p.dialogs.borrow().scene_settings.open,
        "keybindings" => p.dialogs.borrow().keybindings.open,
        "export" => p.dialogs.borrow().export_dialog.open,
        "effect_add" => p.properties.borrow().effect_add.open,
        "easing_editor" => crate::ui::properties::easing_editor::is_open(),
        _ => false,
    }
}

fn set_dialog_open(p: &RegisteredPreview, id: &str, open: bool) {
    match id {
        "system_settings" => p.dialogs.borrow_mut().system_settings.open = open,
        "project_settings" => p.dialogs.borrow_mut().project_settings.open = open,
        "scene_settings" => p.dialogs.borrow_mut().scene_settings.open = open,
        "keybindings" => p.dialogs.borrow_mut().keybindings.set_open(open),
        "export" => p.dialogs.borrow_mut().export_dialog.open = open,
        "effect_add" => p.properties.borrow_mut().effect_add.open = open,
        "easing_editor" => {
            if !open {
                crate::ui::properties::easing_editor::close();
            }
        }
        _ => {}
    }
}

fn show_dialog_contents(ctx: &egui::Context, ui: &mut egui::Ui, p: &RegisteredPreview, id: &str) {
    match id {
        "system_settings" => {
            p.dialogs.borrow_mut().system_settings.show(
                ctx,
                ui,
                &crate::app::state::settings_world(&p.state),
            );
        }
        "project_settings" => {
            p.dialogs
                .borrow_mut()
                .project_settings
                .show(ctx, ui, &p.state);
        }
        "scene_settings" => {
            p.dialogs
                .borrow_mut()
                .scene_settings
                .show(ctx, ui, &p.state);
        }
        "keybindings" => {
            p.dialogs.borrow_mut().keybindings.show(ctx, ui);
        }
        "export" => {
            p.dialogs.borrow_mut().export_dialog.show(ctx, ui, &p.state);
        }
        "effect_add" => {
            p.properties.borrow_mut().show_effect_add(ui, &p.state);
        }
        "easing_editor" => {
            let holder = crate::app::state::active_world(&p.state);
            let mut world = holder.lock().unwrap();
            if !crate::ui::properties::easing_editor::show(ctx, ui, &mut world) {
                crate::ui::properties::easing_editor::close();
            }
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Launcher,
    Project,
}

fn launch_wait_overlay(ctx: &egui::Context, name: &str) {
    let mut open = true;
    elegance::Modal::new("launch_wait_overlay", &mut open)
        .heading(t!("起動中…"))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(elegance::Spinner::new());
                ui.label(name);
            });
        });
}

pub struct NeoUtlApp {
    gpu: Rc<SharedGpu>,
    slot: PreviewSlot,
    launcher: LauncherPanel,
    init_rx: std::sync::mpsc::Receiver<()>,
    init_done: bool,
    pending_project: Option<ProjectMeta>,
    stage: Stage,
}

impl NeoUtlApp {
    fn new(gpu: Rc<SharedGpu>, init_rx: std::sync::mpsc::Receiver<()>) -> Self {
        if let Some(loaded) = crate::ui::system_settings::load_from_disk() {
            crate::app::theme::restore(&loaded.theme_id);
        }
        Self {
            gpu,
            slot: make_preview_slot(),
            launcher: LauncherPanel::new(),
            init_rx,
            init_done: false,
            pending_project: None,
            stage: Stage::Launcher,
        }
    }

    fn poll_init(&mut self) {
        if self.init_done {
            return;
        }
        match self.init_rx.try_recv() {
            Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.init_done = true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
}

impl eframe::App for NeoUtlApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::app::theme::install(&ctx);
        ctx.request_repaint();

        self.poll_init();

        let stage = if self.slot.borrow().is_some() {
            Stage::Project
        } else {
            Stage::Launcher
        };
        self.stage = stage;

        if stage == Stage::Launcher {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("NeoUtl".to_owned()));
            let blocked = self.pending_project.is_some();
            egui::CentralPanel::default().show(ui, |ui| {
                ui.add_enabled_ui(!blocked, |ui| {
                    if let Some(meta) = self.launcher.show(ui) {
                        self.pending_project = Some(meta);
                    }
                });
            });
            if let Some(meta) = &self.pending_project {
                launch_wait_overlay(&ctx, &meta.name);
                if self.init_done {
                    let meta = self.pending_project.take().expect("直前にSomeを確認済み");
                    crate::ui::start_project(meta, self.gpu.clone(), self.slot.clone());
                }
            }
            return;
        }

        let render_state = frame
            .wgpu_render_state()
            .expect("eframeはwgpuバックエンドで起動されている前提");
        let egui_renderer = render_state.renderer.clone();

        let slot_ref = self.slot.borrow();
        let p = slot_ref
            .as_ref()
            .expect("Stage::Projectはslot存在を保証する");

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            crate::app::state::active_project_window_title(&p.state),
        ));
        egui::CentralPanel::default().show(ui, |ui| {
            let mut renderer = egui_renderer.write();
            p.panel
                .borrow_mut()
                .show(ui, &mut renderer, &p.state, &p.dialogs);
        });
        p.dialogs
            .borrow_mut()
            .sync_preview_requests(&p.state, &p.panel);

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("timeline"),
            egui::ViewportBuilder::default().with_title(t!("拡張編集")),
            |ui, _class| {
                let ctx = ui.ctx().clone();
                egui::CentralPanel::default().show(ui, |ui| {
                    p.timeline
                        .borrow_mut()
                        .show(&ctx, ui, &p.state, &p.panel, &(), &p.dialogs);
                });
            },
        );

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("properties"),
            egui::ViewportBuilder::default().with_title(t!("プロパティ")),
            |ui, _class| {
                let ctx = ui.ctx().clone();
                egui::CentralPanel::default().show(ui, |ui| {
                    p.properties.borrow_mut().show(&ctx, ui, &p.state, &p.panel);
                });
            },
        );

        for id in DIALOG_WINDOW_IDS {
            if !dialog_open_state(p, id) {
                continue;
            }
            let mut still_open = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(id),
                egui::ViewportBuilder::default().with_title(dialog_title(id)),
                |ui, _class| {
                    let ctx = ui.ctx().clone();
                    egui::CentralPanel::default().show(ui, |ui| {
                        show_dialog_contents(&ctx, ui, p, id);
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        still_open = false;
                    }
                },
            );
            if !still_open {
                set_dialog_open(p, id, false);
            }
        }
    }
}

pub fn run(
    gpu: Rc<SharedGpu>,
    init_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_inner_size([LAUNCHER_WINDOW_SIZE.0, LAUNCHER_WINDOW_SIZE.1]);
    options.wgpu_options.wgpu_setup =
        egui_wgpu::WgpuSetup::Existing(egui_wgpu::WgpuSetupExisting {
            instance: gpu.instance.clone(),
            adapter: gpu.adapter.clone(),
            device: (*gpu.device).clone(),
            queue: (*gpu.queue).clone(),
        });
    let gpu_for_app = gpu.clone();
    eframe::run_native(
        "NeoUtl",
        options,
        Box::new(move |cc| {
            crate::app::theme::install(&cc.egui_ctx);
            crate::ui::ui_ext::configure_taffy(&cc.egui_ctx);
            egui_material_icons::initialize(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            egui_system_fonts::set_auto(&cc.egui_ctx, egui_system_fonts::FontStyle::Sans);
            neoutl_media_runtime::cache::global().set_redraw_handle(std::sync::Arc::new({
                let ctx = cc.egui_ctx.clone();
                move || ctx.request_repaint()
            }));
            Ok(Box::new(NeoUtlApp::new(gpu_for_app, init_rx)))
        }),
    )?;
    Ok(())
}
