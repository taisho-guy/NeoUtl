// src/ui/mod.rs
use crate::MainWindow;
use crate::TimelineObject;
use crate::ecs::EcsWorld;
use crate::ecs::systems::get_active_render_kinds_system;
use crate::objects::RenderKind;
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
            let frame = (ratio * world.resources.total_frames as f32) as i32;
            world.resources.current_frame = frame.clamp(0, world.resources.total_frames);
            app.set_current_frame(world.resources.current_frame);
        }
    });

    let world_ctrl = world_holder.clone();
    let app_ctrl = app.as_weak();
    app.on_add_object_at(move |ratio, kind_idx| {
        if let Some(app) = app_ctrl.upgrade() {
            let mut world = world_ctrl.lock().unwrap();
            let start = (ratio * world.resources.total_frames as f32) as i32;

            let kind = match kind_idx {
                0 => RenderKind::Tetrahedron,
                _ => RenderKind::Cube,
            };

            world.add_object(start, 90, kind);
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
    let engine_ctrl = engine_holder.clone();
    app.on_request_render(move || {
        let mut engine_lock = engine_ctrl.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            let world = world_ctrl.lock().unwrap();

            let active_kinds = get_active_render_kinds_system(&world);

            engine.render(&active_kinds);

            let imported_image = slint::Image::try_from(engine.texture.clone()).unwrap();
            if let Some(app) = app_weak.upgrade() {
                app.set_video_frame(imported_image);
            }
        }
    });
}

fn sync_world_to_ui(app: &MainWindow, world: &EcsWorld) {
    app.set_total_frames(world.resources.total_frames);
    let slint_objs: Vec<TimelineObject> = world
        .entities
        .iter()
        .zip(world.time_ranges.iter())
        .map(|(&id, &t)| TimelineObject {
            id: id as i32,
            start_frame: t.start_frame,
            end_frame: t.end_frame,
        })
        .collect();
    app.set_objects(slint::ModelRc::new(slint::VecModel::from(slint_objs)));
}
