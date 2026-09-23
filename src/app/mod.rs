pub mod shortcuts;
pub mod splash;
pub mod state;
pub mod theme;
pub mod update;

use crate::app::state::SharedAppState;
use crate::infra::gpu_shared::SharedGpu;
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

struct DialogWindow {
    id: &'static str,
    title: &'static str,
}

const DIALOG_WINDOWS: &[DialogWindow] = &[
    DialogWindow {
        id: "system_settings",
        title: "System Settings",
    },
    DialogWindow {
        id: "project_settings",
        title: "Project Settings",
    },
    DialogWindow {
        id: "scene_settings",
        title: "Scene Settings",
    },
    DialogWindow {
        id: "keybindings",
        title: "Keybindings",
    },
    DialogWindow {
        id: "export",
        title: "Export",
    },
    DialogWindow {
        id: "effect_add",
        title: "Add Effect",
    },
    DialogWindow {
        id: "easing_editor",
        title: "Easing Editor",
    },
];

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

pub struct NeoUtlApp {
    gpu: Rc<SharedGpu>,
    slot: PreviewSlot,
    launcher: LauncherPanel,
    init_rx: std::sync::mpsc::Receiver<()>,
    init_done: bool,
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
        }
    }
}

impl eframe::App for NeoUtlApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        crate::app::theme::install(&ctx);
        ctx.request_repaint();

        if !self.init_done {
            match self.init_rx.try_recv() {
                Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.init_done = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.add(egui::Image::new(crate::app::splash::SOURCE.clone()));
                            });
                        });
                    return;
                }
            }
        }

        if self.slot.borrow().is_none() {
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(meta) = self.launcher.show(ui) {
                    crate::ui::start_project(meta, self.gpu.clone(), self.slot.clone());
                }
            });
            return;
        }

        let render_state = frame
            .wgpu_render_state()
            .expect("eframeはwgpuバックエンドで起動されている前提");
        let egui_renderer = render_state.renderer.clone();

        let slot_ref = self.slot.borrow();
        let p = slot_ref.as_ref().expect("slot存在確認済み");

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            crate::app::state::active_project_window_title(&p.state),
        ));

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("preview"),
            egui::ViewportBuilder::default().with_title("Preview"),
            |ui, _class| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut renderer = egui_renderer.write();
                    p.panel
                        .borrow_mut()
                        .show(ui, &mut renderer, &p.state, &p.dialogs);
                });
            },
        );

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("timeline"),
            egui::ViewportBuilder::default().with_title("Timeline"),
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
            egui::ViewportBuilder::default().with_title("Properties"),
            |ui, _class| {
                let ctx = ui.ctx().clone();
                egui::CentralPanel::default().show(ui, |ui| {
                    p.properties.borrow_mut().show(&ctx, ui, &p.state, &p.panel);
                });
            },
        );

        for dialog in DIALOG_WINDOWS {
            if !dialog_open_state(p, dialog.id) {
                continue;
            }
            let mut still_open = true;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(dialog.id),
                egui::ViewportBuilder::default().with_title(dialog.title),
                |ui, _class| {
                    let ctx = ui.ctx().clone();
                    egui::CentralPanel::default().show(ui, |ui| {
                        show_dialog_contents(&ctx, ui, p, dialog.id);
                    });
                    if ui.input(|i| i.viewport().close_requested()) {
                        still_open = false;
                    }
                },
            );
            if !still_open {
                set_dialog_open(p, dialog.id, false);
            }
        }
    }
}

pub fn run(
    gpu: Rc<SharedGpu>,
    init_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut options = eframe::NativeOptions::default();
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
