// src/ui/properties.rs
use crate::ecs::EcsWorld;
use crate::renderer::RenderEngine;
use crate::{PreviewWindow, PropertiesWindow};
use slint::Weak;
use std::sync::{Arc, Mutex};

pub fn setup(
    props: &PropertiesWindow,
    preview_weak: Weak<PreviewWindow>,
    world_holder: Arc<Mutex<EcsWorld>>,
    engine_holder: Arc<Mutex<Option<RenderEngine>>>,
) {
    {
        let (wc, pw) = (world_holder.clone(), preview_weak.clone());
        props.on_set_fps(move |fps| {
            wc.lock().unwrap().set_fps(fps as u32);
            if let Some(p) = pw.upgrade() {
                p.set_fps(fps);
            }
        });
    }

    {
        let (wc, pw, eh) = (
            world_holder.clone(),
            preview_weak.clone(),
            engine_holder.clone(),
        );
        props.on_set_resolution(move |width, height| {
            wc.lock()
                .unwrap()
                .set_resolution(width as u32, height as u32);
            if let Some(p) = pw.upgrade() {
                p.set_res_width(width);
                p.set_res_height(height);
            }
            if let Some(ref mut engine) = *eh.lock().unwrap() {
                engine.resize_render_target(width as u32, height as u32);
            }
        });
    }
}
