// src/ui/mod.rs
mod preview;
pub mod properties;
mod timeline;

use crate::ecs::EcsWorld;
use crate::renderer::RenderEngine;
use crate::{PreviewWindow, PropertiesWindow, TimelineWindow};
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

pub fn setup_ui_callbacks(
    preview_win: &PreviewWindow,
    timeline_win: &TimelineWindow,
    props_win: &PropertiesWindow,
    world_holder: Arc<Mutex<EcsWorld>>,
    engine_holder: Arc<Mutex<Option<RenderEngine>>>,
) {
    preview::setup(
        preview_win,
        timeline_win.as_weak(),
        world_holder.clone(),
        engine_holder.clone(),
    );
    timeline::setup(
        timeline_win,
        preview_win.as_weak(),
        props_win.as_weak(),
        world_holder.clone(),
    );
    properties::setup(props_win, world_holder);
}
