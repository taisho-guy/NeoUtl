// src/ui/mod.rs
use crate::MainWindow;
use crate::ecs::EcsWorld;
use crate::ecs::components::TextContent;
use crate::ecs::systems::get_active_objects_system;
use crate::objects::registry;
use crate::renderer::RenderEngine;
use slint::ComponentHandle;
use std::sync::{Arc, Mutex};

pub fn setup_ui_callbacks(
    app: &MainWindow,
    world_holder: Arc<Mutex<EcsWorld>>,
    engine_holder: Arc<Mutex<Option<RenderEngine>>>,
) {
    let app_weak = app.as_weak();

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_seek_timeline(move |ratio| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            let total = world.total_frames();
            let frame = (ratio * total as f32) as i32;
            let clamped = frame.clamp(0, total);
            world.set_current_frame(clamped);
            app.set_current_frame(clamped);
        }
    });

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_add_object_at(move |ratio, kind_idx| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            let start = (ratio * world.total_frames() as f32) as i32;
            let kind_id = kind_idx as u32;
            let text = registry()
                .get(kind_idx as usize)
                .filter(|p| p.name == "Text")
                .map(|_| TextContent::default());
            world.add_object(start, 90, kind_id, text);
            sync_world_to_ui(&app, &world);
        }
    });

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_delete_object(move |id| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            world.delete_object(id as usize);
            sync_world_to_ui(&app, &world);
        }
    });

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_move_object(move |id, new_start_ratio| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            let total = world.total_frames();
            let new_start = (new_start_ratio * total as f32) as i32;
            let new_start = new_start.clamp(0, total);
            world.move_object(id as usize, new_start);
            sync_world_to_ui(&app, &world);
        }
    });

    let world_ctrl = world_holder.clone();
    app.on_find_object_at(move |ratio| {
        let world = world_ctrl.lock().unwrap();
        world.find_object_at(ratio)
    });

    let world_ctrl = world_holder.clone();
    app.on_get_object_start_ratio(move |id| {
        let world = world_ctrl.lock().unwrap();
        world.get_object_start_ratio(id as usize)
    });

    let world_ctrl = world_holder.clone();
    let engine_ctrl = engine_holder.clone();
    app.on_request_render(move || {
        let mut engine_lock = engine_ctrl.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            let world = world_ctrl.lock().unwrap();
            let active = get_active_objects_system(&world);
            let proj = world.get_project();
            engine.render(&active, &proj);
            let img = slint::Image::try_from(engine.texture.clone()).unwrap();
            if let Some(app) = app_weak.upgrade() {
                app.set_video_frame(img);
            }
        }
    });

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_set_fps(move |fps| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            world.set_fps(fps as u32);
            app.set_fps(fps);
        }
    });

    let world_ctrl = world_holder.clone();
    let engine_ctrl = engine_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_set_resolution(move |width, height| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            world.set_resolution(width as u32, height as u32);
            app.set_res_width(width);
            app.set_res_height(height);
            if let Some(ref mut engine) = *engine_ctrl.lock().unwrap() {
                engine.resize_render_target(width as u32, height as u32);
            }
        }
    });
}

fn sync_world_to_ui(app: &MainWindow, world: &EcsWorld) {
    app.set_total_frames(world.total_frames());
    let slint_objs = world.get_timeline_objects();
    app.set_objects(slint::ModelRc::new(slint::VecModel::from(slint_objs)));
}
