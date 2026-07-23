use crate::app_state::{self, SharedAppState};
use crate::ecs::SceneSettings;
use crate::project;
use crate::{SceneSettingsWindow, TimelineWindow};
use slint::{ComponentHandle, Weak};

pub fn setup(
    window: &SceneSettingsWindow,
    state: SharedAppState,
    timeline_weak: Weak<TimelineWindow>,
) {
    {
        let state = state.clone();
        let window_weak = window.as_weak();
        let timeline_weak = timeline_weak.clone();
        window.on_confirm(move || {
            let Some(w) = window_weak.upgrade() else {
                return;
            };
            let settings = SceneSettings {
                name: w.get_scene_name().to_string(),
                width: w.get_scene_width().max(1) as u32,
                height: w.get_scene_height().max(1) as u32,
                fps: w.get_scene_fps().max(1.0) as u32,
                grid_mode: w.get_grid_mode(),
                grid_bpm: w.get_grid_bpm(),
                grid_offset: w.get_grid_offset(),
                grid_interval: w.get_grid_interval(),
                grid_subdivision: w.get_grid_subdivision(),
                enable_snap: w.get_enable_snap(),
                magnetic_snap_range: w.get_magnetic_snap_range(),
            };

            let world_holder = app_state::active_world(&state);
            app_state::snapshot_before_edit(&state);
            let mut world = world_holder.lock().unwrap();

            let scene_id = if w.get_is_creation_mode() {
                let id = world.add_scene(settings.name.clone());
                world.switch_scene(id);
                id
            } else {
                w.get_target_scene_id()
            };
            world.update_scene_settings(scene_id, settings);
            let _ = project::save_from_world(&world);
            drop(world);

            crate::ui::timeline::sync_active_session(&state, &timeline_weak);
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

/// 新規作成モードでダイアログを開く。幅・高さ・FPSはプロジェクトの現在値を初期値にする。
/// グリッド・スナップ既定値はSceneMeta::new_with_defaults（=システム設定のdefault_snap/
/// magnetic_snap_rangeを反映したSceneMeta既定値）のみを参照し、本関数側で数値を重複定義しない。
pub fn open_for_create(window: &SceneSettingsWindow, state: &SharedAppState) {
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let project = world.get_project();
    let count = world.scenes().len();
    drop(world);

    let settings_holder = app_state::settings_world(state);
    let system_settings = settings_holder.lock().unwrap().get_system_settings();
    let defaults = crate::ecs::resources::SceneMeta::new_with_defaults(
        -1,
        "",
        system_settings.default_snap,
        system_settings.magnetic_snap_range,
    );

    window.set_is_creation_mode(true);
    window.set_target_scene_id(-1);
    window.set_scene_name(format!("Scene {}", count + 1).into());
    window.set_scene_width(project.width as i32);
    window.set_scene_height(project.height as i32);
    window.set_scene_fps(project.fps as f32);
    window.set_enable_snap(defaults.enable_snap);
    window.set_magnetic_snap_range(defaults.magnetic_snap_range);
    window.set_grid_mode(defaults.grid_mode);
    window.set_grid_bpm(defaults.grid_bpm);
    window.set_grid_offset(defaults.grid_offset);
    window.set_grid_interval(defaults.grid_interval);
    window.set_grid_subdivision(defaults.grid_subdivision);
    let _ = window.show();
    window.window().request_redraw();
}

/// 既存シーンの編集モードでダイアログを開き、現在値を反映する。
pub fn open_for_edit(window: &SceneSettingsWindow, state: &SharedAppState, scene_id: i32) {
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    let Some(s) = world.get_scene(scene_id) else {
        return;
    };
    drop(world);

    window.set_is_creation_mode(false);
    window.set_target_scene_id(scene_id);
    window.set_scene_name(s.name.into());
    window.set_scene_width(s.width as i32);
    window.set_scene_height(s.height as i32);
    window.set_scene_fps(s.fps as f32);
    window.set_enable_snap(s.enable_snap);
    window.set_magnetic_snap_range(s.magnetic_snap_range);
    window.set_grid_mode(s.grid_mode);
    window.set_grid_bpm(s.grid_bpm);
    window.set_grid_offset(s.grid_offset);
    window.set_grid_interval(s.grid_interval);
    window.set_grid_subdivision(s.grid_subdivision);
    let _ = window.show();
    window.window().request_redraw();
}
