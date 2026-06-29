// src/ui/preview.rs
use crate::PreviewWindow;
use crate::ecs::{EcsWorld, systems::get_active_objects_system};
use crate::renderer::RenderEngine;
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

pub fn setup(
    preview: &PreviewWindow,
    world_holder: Arc<Mutex<EcsWorld>>,
    engine_holder: Arc<Mutex<Option<RenderEngine>>>,
) {
    let preview_weak = preview.as_weak();
    preview.on_request_render(move || {
        let mut engine_lock = engine_holder.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            let world = world_holder.lock().unwrap();
            let active = get_active_objects_system(&world);
            let proj = world.get_project();
            engine.render(&active, &proj);
            let img = slint::Image::try_from(engine.texture.clone()).unwrap();
            if let Some(p) = preview_weak.upgrade() {
                p.set_video_frame(img);
            }
        }
    });
}
