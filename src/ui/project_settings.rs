// src/ui/project_settings.rs
use crate::app_state::{self, SharedAppState};
use crate::project;
use crate::{PreviewWindow, ProjectSettingsWindow};
use slint::{ComponentHandle, Weak};

pub fn setup(
    window: &ProjectSettingsWindow,
    state: SharedAppState,
    preview_weak: Weak<PreviewWindow>,
) {
    {
        let state = state.clone();
        let window_weak = window.as_weak();
        let preview_weak = preview_weak.clone();
        window.on_confirm(move || {
            let Some(w) = window_weak.upgrade() else {
                return;
            };
            let name = w.get_project_name().to_string();
            let sample_rate = w.get_audio_sample_rate().max(1) as u32;
            let channels = w.get_audio_channels().clamp(1, 8) as u32;

            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let dir = world
                .get_project()
                .dir
                .unwrap_or_else(project::projects_dir);
            world.set_project_meta(name.clone(), dir);
            world.set_audio_format(sample_rate, channels);
            let _ = project::save_from_world(&world);
            drop(world);

            {
                let mut s = state.lock().unwrap();
                let active = s.active;
                s.sessions[active].meta.name = name;
            }

            if let Some(p) = preview_weak.upgrade() {
                crate::ui::preview::sync_project_tabs(&state, &p);
            }

            let _ = w.hide();
        });
    }

    window.on_cancel({
        let window_weak = window.as_weak();
        move || {
            if let Some(w) = window_weak.upgrade() {
                let _ = w.hide();
            }
        }
    });
}

/// アクティブプロジェクトの現在値を反映してダイアログを開く。
pub fn open(window: &ProjectSettingsWindow, state: &SharedAppState) {
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let project = world.get_project();
    drop(world);

    window.set_project_name(project.name.into());
    window.set_audio_sample_rate(project.audio_sample_rate as i32);
    window.set_audio_channels(project.audio_channels as i32);
    let _ = window.show();
}
