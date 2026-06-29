// src/ui/timeline.rs
use crate::ecs::{EcsWorld, TimelineData, components::TextContent};
use crate::objects::registry;
use crate::{PreviewWindow, TimelineObject, TimelineWindow};
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::sync::{Arc, Mutex};

pub fn setup(
    timeline: &TimelineWindow,
    preview_weak: Weak<PreviewWindow>,
    world_holder: Arc<Mutex<EcsWorld>>,
) {
    {
        let (wc, tw, pw) = (
            world_holder.clone(),
            timeline.as_weak(),
            preview_weak.clone(),
        );
        timeline.on_seek_timeline(move |ratio| {
            let mut world = wc.lock().unwrap();
            let clamped =
                ((ratio * world.total_frames() as f32) as i32).clamp(0, world.total_frames());
            world.set_current_frame(clamped);
            if let Some(t) = tw.upgrade() {
                t.set_current_frame(clamped);
            }
            if let Some(p) = pw.upgrade() {
                p.set_current_frame(clamped);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_add_object_at(move |ratio, kind_idx| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                let start = (ratio * world.total_frames() as f32) as i32;
                let text = registry()
                    .get(kind_idx as usize)
                    .filter(|p| p.name == "Text")
                    .map(|_| TextContent::default());
                world.add_object(start, 90, kind_idx as u32, text);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_delete_object(move |id| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                world.delete_object(id as usize);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_move_object(move |id, ratio| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                let total = world.total_frames();
                let new_start = ((ratio * total as f32) as i32).clamp(0, total);
                world.move_object(id as usize, new_start);
                sync(&t, &world);
            }
        });
    }

    {
        let wc = world_holder.clone();
        timeline.on_find_object_at(move |ratio| wc.lock().unwrap().find_object_at(ratio));
    }

    {
        let wc = world_holder.clone();
        timeline.on_get_object_start_ratio(move |id| {
            wc.lock().unwrap().get_object_start_ratio(id as usize)
        });
    }
}

fn to_slint(data: &TimelineData) -> TimelineObject {
    let label = registry()
        .get(data.kind as usize)
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown")
        .into();
    TimelineObject {
        id: data.id,
        start_frame: data.start_frame,
        end_frame: data.end_frame,
        kind: data.kind,
        label,
    }
}

fn sync(timeline: &TimelineWindow, world: &EcsWorld) {
    timeline.set_total_frames(world.total_frames());
    let objs: Vec<TimelineObject> = world.get_timeline_objects().iter().map(to_slint).collect();
    timeline.set_objects(ModelRc::new(VecModel::from(objs)));
}
