pub mod dialogs;
pub mod effect_add_dialog;
pub mod effect_catalog;
pub mod export_dialog;
pub mod font_stack;
pub mod keybindings;
pub mod launcher;
pub mod preview;
pub mod project_settings;
pub mod properties;
pub mod scene_settings;
pub mod system_settings;
pub mod timeline;
pub mod types;
pub mod ui_ext;
pub mod workspace_bridge;

use crate::app_state::{self, AppState, ProjectSession, SharedAppState};
use crate::project::ProjectMeta;
use dialogs::DialogSet;
use launcher::LauncherPanel;
use preview::{LegacyWindows, PreviewPanel};
use properties::PropertiesPanel;
use std::sync::{Arc, Mutex, OnceLock};
use timeline::TimelineWindow;

pub struct UiState {
    pub app_state: Option<SharedAppState>,
    pub launcher: LauncherPanel,
    pub preview: Option<PreviewPanel>,
    pub timeline: Option<TimelineWindow>,
    pub properties: Option<PropertiesPanel>,
    pub dialogs: Option<DialogSet>,
    pub project_open: bool,
}

static UI_STATE: OnceLock<Mutex<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static Mutex<UiState> {
    UI_STATE.get_or_init(|| {
        Mutex::new(UiState {
            app_state: None,
            launcher: LauncherPanel::new(),
            preview: None,
            timeline: None,
            properties: None,
            dialogs: None,
            project_open: false,
        })
    })
}

pub fn open_project(meta: ProjectMeta, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
    let state = AppState::new(ProjectSession::new(meta));
    let mut preview = PreviewPanel::new(device, queue, LegacyWindows {});
    preview.sync_active_session(&state);

    let dialogs = DialogSet::new(app_state::settings_world(&state).clone());
    let timeline = TimelineWindow::new();
    let properties = PropertiesPanel::new();

    let mut ui = ui_state().lock().unwrap();
    ui.app_state = Some(state);
    ui.preview = Some(preview);
    ui.timeline = Some(timeline);
    ui.properties = Some(properties);
    ui.dialogs = Some(dialogs);
    ui.project_open = true;
}
