// src/ui/mod.rs
pub mod launcher;
mod preview;
pub mod properties;
pub mod system_settings;
mod timeline;

use crate::app_state::{self, AppState, ProjectSession, SharedAppState};
use crate::project::ProjectMeta;
use crate::{
    LauncherWindow, PreviewWindow, PropertiesWindow, SystemSettingsWindow, TimelineWindow,
};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;

/// 本体ウィンドウ一式。最初のプロジェクト確定時に一度だけ生成する。
struct AppHandles {
    preview: PreviewWindow,
    timeline: TimelineWindow,
    props: PropertiesWindow,
    #[allow(dead_code)]
    settings: SystemSettingsWindow,
}

/// ランチャーのコールバックを配線する。
/// 本体ウィンドウ群は最初のプロジェクト確定時に生成し、以後は使い回す。
/// 2件目以降のプロジェクトはセッション追加とタブ反映のみ行う。
pub fn install(launcher: &LauncherWindow) {
    let state_slot: Rc<RefCell<Option<SharedAppState>>> = Rc::new(RefCell::new(None));
    let handles_slot: Rc<RefCell<Option<AppHandles>>> = Rc::new(RefCell::new(None));

    let add_session: Rc<dyn Fn(ProjectMeta)> = {
        let state_slot = state_slot.clone();
        let handles_slot = handles_slot.clone();
        let launcher_weak = launcher.as_weak();

        Rc::new(move |meta: ProjectMeta| {
            let session = ProjectSession::new(meta);
            let is_first = state_slot.borrow().is_none();

            if is_first {
                let state = AppState::new(session);
                *state_slot.borrow_mut() = Some(state.clone());

                match build_main_windows(&state, &launcher_weak) {
                    Ok(handles) => {
                        let _ = handles.preview.show();
                        let _ = handles.timeline.show();
                        let _ = handles.props.show();
                        *handles_slot.borrow_mut() = Some(handles);
                    }
                    Err(err) => {
                        eprintln!("[NeoUtl] 本体ウィンドウ生成失敗: {err}");
                        return;
                    }
                }
            } else {
                let state = state_slot.borrow().as_ref().unwrap().clone();
                {
                    let mut s = state.lock().unwrap();
                    s.sessions.push(session);
                    s.active = s.sessions.len() - 1;
                }
                if let Some(h) = handles_slot.borrow().as_ref() {
                    preview::sync_active_session(
                        &state,
                        &h.preview.as_weak(),
                        &h.timeline.as_weak(),
                        &h.props.as_weak(),
                    );
                    let _ = h.preview.show();
                    let _ = h.timeline.show();
                    let _ = h.props.show();
                }
            }

            if let Some(l) = launcher_weak.upgrade() {
                let _ = l.hide();
            }
        })
    };

    launcher::setup(launcher, add_session);
}

fn build_main_windows(
    state: &SharedAppState,
    launcher_weak: &slint::Weak<LauncherWindow>,
) -> Result<AppHandles, Box<dyn std::error::Error>> {
    let preview = PreviewWindow::new()?;
    let timeline = TimelineWindow::new()?;
    let props = PropertiesWindow::new()?;
    let settings = SystemSettingsWindow::new()?;

    let gpu_slot: preview::GpuSlot = Rc::new(RefCell::new(None));
    preview::install_rendering_notifier(&preview, gpu_slot.clone());

    system_settings::setup(&settings, app_state::active_world(state));
    preview::setup(
        &preview,
        timeline.as_weak(),
        props.as_weak(),
        settings.as_weak(),
        state.clone(),
        gpu_slot,
    );
    timeline::setup(&timeline, preview.as_weak(), props.as_weak(), state.clone());
    properties::setup(&props, state.clone());

    // 本体ウィンドウの「新規プロジェクト」「プロジェクトを開く」は
    // ランチャーを再表示してプロジェクトタブ追加の入口として使う。
    preview.on_new_project({
        let launcher_weak = launcher_weak.clone();
        move || {
            if let Some(l) = launcher_weak.upgrade() {
                let _ = l.show();
            }
        }
    });
    preview.on_open_project({
        let launcher_weak = launcher_weak.clone();
        move || {
            if let Some(l) = launcher_weak.upgrade() {
                let _ = l.show();
            }
        }
    });

    Ok(AppHandles {
        preview,
        timeline,
        props,
        settings,
    })
}
