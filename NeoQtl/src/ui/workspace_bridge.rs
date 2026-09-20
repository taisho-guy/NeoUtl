use crate::app_state;
use crate::ui;

#[cxx::bridge(namespace = "neoqtl")]
mod ffi {
    #[derive(Clone, Debug, Default)]
    struct TabData {
        name: String,
        has_unsaved: bool,
    }

    extern "Rust" {
        fn workspace_tabs() -> Vec<TabData>;
        fn workspace_current_index() -> i32;
        fn workspace_set_current_index(index: i32);
        fn workspace_new_project() -> bool;
        fn workspace_load_project(path: &str) -> bool;
        fn workspace_close_project(index: i32) -> bool;
    }
}

use ffi::TabData;

macro_rules! with_state_or {
    ($default:expr) => {{
        let guard = ui::ui_state().lock().unwrap();
        let Some(state) = guard.app_state.clone() else {
            return $default;
        };
        state
    }};
}

pub fn workspace_tabs() -> Vec<TabData> {
    let state = with_state_or!(Vec::new());
    let s = state.lock().unwrap();
    s.sessions
        .iter()
        .map(|session| TabData {
            name: session.meta.name.clone(),
            has_unsaved: session.dirty,
        })
        .collect()
}

pub fn workspace_current_index() -> i32 {
    let state = with_state_or!(0);
    state.lock().unwrap().active as i32
}

pub fn workspace_set_current_index(index: i32) {
    let state = with_state_or!(());
    let mut s = state.lock().unwrap();
    if index >= 0 && (index as usize) < s.sessions.len() {
        s.active = index as usize;
    }
}

pub fn workspace_new_project() -> bool {
    let state = with_state_or!(false);
    app_state::new_project_session(&state).is_ok()
}

pub fn workspace_load_project(path: &str) -> bool {
    let state = with_state_or!(false);
    app_state::open_project_session(&state, std::path::Path::new(path)).is_ok()
}

pub fn workspace_close_project(index: i32) -> bool {
    let state = with_state_or!(false);
    if index < 0 {
        return false;
    }
    app_state::close_session(&state, index as usize).is_ok()
}
