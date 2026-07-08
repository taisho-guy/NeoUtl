// src/ui/preview.rs
use crate::PreviewWindow;
use crate::SystemSettingsWindow;
use crate::TimelineWindow;
use crate::ecs::{EcsWorld, systems::get_active_objects_system};
use crate::renderer::RenderEngine;
use slint::{ComponentHandle, Weak};
use std::sync::{Arc, Mutex};

fn apply_frame(
    frame: i32,
    world_holder: &Arc<Mutex<EcsWorld>>,
    preview_weak: &Weak<PreviewWindow>,
    timeline_weak: &Weak<TimelineWindow>,
) {
    let mut world = world_holder.lock().unwrap();
    let clamped = frame.clamp(0, world.total_frames());
    world.set_current_frame(clamped);
    if let Some(p) = preview_weak.upgrade() {
        p.set_current_frame(clamped);
    }
    if let Some(t) = timeline_weak.upgrade() {
        t.set_current_frame(clamped);
    }
}

pub fn setup(
    preview: &PreviewWindow,
    timeline_weak: Weak<TimelineWindow>,
    settings_weak: Weak<SystemSettingsWindow>,
    world_holder: Arc<Mutex<EcsWorld>>,
    engine_holder: Arc<Mutex<Option<RenderEngine>>>,
) {
    let preview_weak = preview.as_weak();

    preview.on_request_render({
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let world_holder = world_holder.clone();
        move || {
            {
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
            }

            if let Some(p) = preview_weak.upgrade() {
                if p.get_is_playing() {
                    let total = p.get_total_frames();
                    let step = (p.get_speed_percent().max(10) + 50) / 100;
                    let next = p.get_current_frame() + step.max(1);
                    if next >= total {
                        p.set_is_playing(false);
                        apply_frame(total, &world_holder, &preview_weak, &timeline_weak);
                    } else {
                        apply_frame(next, &world_holder, &preview_weak, &timeline_weak);
                    }
                }
            }
        }
    });

    preview.on_toggle_play({
        let preview_weak = preview_weak.clone();
        move || {
            if let Some(p) = preview_weak.upgrade() {
                let playing = !p.get_is_playing();
                p.set_is_playing(playing);
            }
        }
    });

    preview.on_seek({
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let world_holder = world_holder.clone();
        move |frame| {
            apply_frame(frame, &world_holder, &preview_weak, &timeline_weak);
        }
    });

    preview.on_step_frame({
        let preview_weak = preview_weak.clone();
        let timeline_weak = timeline_weak.clone();
        let world_holder = world_holder.clone();
        move |delta| {
            if let Some(p) = preview_weak.upgrade() {
                let next = p.get_current_frame() + delta;
                apply_frame(next, &world_holder, &preview_weak, &timeline_weak);
            }
        }
    });

    preview.on_set_speed({
        let preview_weak = preview_weak.clone();
        move |percent| {
            if let Some(p) = preview_weak.upgrade() {
                p.set_speed_percent(percent.clamp(10, 400));
            }
        }
    });

    preview.on_new_project(|| {});
    preview.on_open_project(|| {});
    preview.on_save_project(|| {});
    preview.on_save_project_as(|| {});
    preview.on_export_media(|| {});
    preview.on_undo(|| {});
    preview.on_redo(|| {});
    preview.on_show_timeline(|| {});
    preview.on_show_properties(|| {});

    preview.on_show_system_settings({
        let settings_weak = settings_weak.clone();
        move || {
            if let Some(w) = settings_weak.upgrade() {
                let _ = w.show();
            }
        }
    });

    preview.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}
