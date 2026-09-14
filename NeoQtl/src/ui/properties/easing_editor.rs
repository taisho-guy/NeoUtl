use crate::ecs::EcsWorld;
use crate::ecs::components::ParamAccess;
use crate::ecs::types::{Keyframe, Value};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackTarget {
    Object {
        object_id: usize,
        key: String,
    },
    Effect {
        object_id: usize,
        effect_index: usize,
        key: String,
    },
}

#[derive(Clone, Debug)]
pub struct EditorState {
    pub target: TrackTarget,
    pub label: String,
    pub selected_frame: Option<i32>,
}

static ACTIVE: Mutex<Option<EditorState>> = Mutex::new(None);

pub fn toggle(target: TrackTarget, label: &str) {
    let mut guard = ACTIVE.lock().unwrap();
    let already_this = guard.as_ref().is_some_and(|s| s.target == target);
    *guard = if already_this {
        None
    } else {
        Some(EditorState {
            target,
            label: label.to_owned(),
            selected_frame: None,
        })
    };
}

pub fn is_open() -> bool {
    ACTIVE.lock().unwrap().is_some()
}

pub fn close() {
    *ACTIVE.lock().unwrap() = None;
}

pub fn current_state() -> Option<EditorState> {
    ACTIVE.lock().unwrap().clone()
}
