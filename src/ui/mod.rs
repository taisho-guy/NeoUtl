pub mod dialogs;
pub mod effect_add_dialog;
pub mod effect_catalog;
pub mod export_dialog;
pub mod keybindings;
pub mod launcher;
pub mod preview;
pub mod project_settings;
pub mod properties;
pub mod scene_settings;
pub mod system_settings;
pub mod timeline;
pub mod types;

use crate::app_state::{AppState, ProjectSession};
use crate::egui_loop::PreviewSlot;
use crate::gpu_shared::SharedGpu;
use crate::project;
use preview::{LegacyWindows, PreviewPanel};
use properties::PropertiesPanel;
use std::cell::RefCell;
use std::rc::Rc;
use timeline::TimelineWindow;

pub fn install(gpu: Rc<SharedGpu>, slot: PreviewSlot) {
    let Some(meta) = project::list_projects().into_iter().next() else {
        return;
    };
    start_project(meta, gpu, slot);
}

pub fn start_project(meta: project::ProjectMeta, gpu: Rc<SharedGpu>, slot: PreviewSlot) {
    let state = AppState::new(ProjectSession::new(meta));
    let panel = Rc::new(RefCell::new(PreviewPanel::new(
        gpu.device.clone(),
        gpu.queue.clone(),
        LegacyWindows {},
    )));
    panel.borrow_mut().sync_active_session(&state);
    let dialogs = Rc::new(RefCell::new(dialogs::DialogSet::new(
        crate::app_state::settings_world(&state).clone(),
    )));
    let timeline = Rc::new(RefCell::new(TimelineWindow::new()));
    timeline.borrow_mut().open = true;
    let properties = Rc::new(RefCell::new(PropertiesPanel::new()));
    crate::egui_loop::set_preview(&slot, panel, dialogs, timeline, properties, state);
}
