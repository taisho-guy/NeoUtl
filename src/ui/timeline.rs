// src/ui/timeline.rs
use crate::ecs::{EcsWorld, TimelineData, components::TextContent};
use crate::objects::registry;
use crate::{LayerState, PreviewWindow, PropertiesWindow, TimelineObject, TimelineWindow};
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};
use std::sync::{Arc, Mutex};

pub fn setup(
    timeline: &TimelineWindow,
    preview_weak: Weak<PreviewWindow>,
    props_weak: Weak<PropertiesWindow>,
    world_holder: Arc<Mutex<EcsWorld>>,
) {
    {
        let (wc, tw, pw) = (
            world_holder.clone(),
            timeline.as_weak(),
            preview_weak.clone(),
        );
        timeline.on_seek_timeline(move |frame| {
            let mut world = wc.lock().unwrap();
            let clamped = frame.clamp(0, world.total_frames());
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
        timeline.on_add_object_at(move |frame, layer, kind_idx| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                let text = registry()
                    .get(kind_idx as usize)
                    .filter(|p| p.name == "Text")
                    .map(|_| TextContent::default());
                world.add_object(frame.max(0), 90, kind_idx as u32, layer.max(0), text);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_delete_object(move |id| {
            if id < 0 {
                return;
            }
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                world.delete_object(id as usize);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw, pw) = (world_holder.clone(), timeline.as_weak(), props_weak.clone());
        timeline.on_select_object(move |id| {
            if let Some(t) = tw.upgrade() {
                let objs = t.get_objects();
                let updated: Vec<TimelineObject> = objs
                    .iter()
                    .map(|mut o| {
                        o.selected = o.id == id;
                        o
                    })
                    .collect();
                t.set_objects(ModelRc::new(VecModel::from(updated)));
            }
            if let Some(p) = pw.upgrade() {
                let world = wc.lock().unwrap();
                crate::ui::properties::select_object(&p, &world, id);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_move_object(move |id, start, layer| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                world.move_object(id as usize, start, layer);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_resize_object(move |id, start, end| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                world.resize_object(id as usize, start, end);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_set_zoom(move |scale| {
            let mut world = wc.lock().unwrap();
            world.set_zoom(scale);
            if let Some(t) = tw.upgrade() {
                t.set_zoom_scale(world.zoom());
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_toggle_layer_visible(move |layer| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                let current = world.layer_states();
                let visible = current.get(layer as usize).map(|s| s.0).unwrap_or(true);
                world.set_layer_visible(layer as usize, !visible);
                sync(&t, &world);
            }
        });
    }

    {
        let (wc, tw) = (world_holder.clone(), timeline.as_weak());
        timeline.on_toggle_layer_locked(move |layer| {
            if let Some(t) = tw.upgrade() {
                let mut world = wc.lock().unwrap();
                let current = world.layer_states();
                let locked = current.get(layer as usize).map(|s| s.1).unwrap_or(false);
                world.set_layer_locked(layer as usize, !locked);
                sync(&t, &world);
            }
        });
    }

    {
        let world = world_holder.lock().unwrap();
        sync(timeline, &world);
        timeline.set_zoom_scale(world.zoom());
        timeline.set_layer_count(world.layer_count());
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
        layer: data.layer,
        label,
        selected: false,
    }
}

fn sync(timeline: &TimelineWindow, world: &EcsWorld) {
    timeline.set_total_frames(world.total_frames());

    let selected_id = timeline
        .get_objects()
        .iter()
        .find(|o| o.selected)
        .map(|o| o.id);

    let objs: Vec<TimelineObject> = world
        .get_timeline_objects()
        .iter()
        .map(to_slint)
        .map(|mut o| {
            o.selected = Some(o.id) == selected_id;
            o
        })
        .collect();
    timeline.set_objects(ModelRc::new(VecModel::from(objs)));

    let states: Vec<LayerState> = world
        .layer_states()
        .iter()
        .map(|&(visible, locked)| LayerState { visible, locked })
        .collect();
    timeline.set_layer_states(ModelRc::new(VecModel::from(states)));
}
