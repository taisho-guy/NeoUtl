// src/ui/timeline.rs
use crate::app_state::{self, SharedAppState};
use crate::ecs::{EcsWorld, components::TextContent};
use crate::objects::registry;
use crate::{
    LayerState, ObjectKindItem, PreviewWindow, PropertiesWindow, SceneSettingsWindow, SceneTabItem,
    TimelineObject, TimelineWindow,
};
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

pub fn setup(
    timeline: &TimelineWindow,
    preview_weak: Weak<PreviewWindow>,
    props_weak: Weak<PropertiesWindow>,
    scene_settings_weak: Weak<SceneSettingsWindow>,
    state: SharedAppState,
) {
    let kinds: Vec<ObjectKindItem> = registry()
        .iter()
        .enumerate()
        .map(|(kind_id, plugin)| ObjectKindItem {
            kind: kind_id as i32,
            name: plugin.name.clone().into(),
        })
        .collect();
    timeline.set_available_kinds(ModelRc::new(VecModel::from(kinds)));

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), preview_weak.clone());
        timeline.on_seek_timeline(move |frame| {
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            let clamped = frame.clamp(0, world.total_frames());
            world.set_current_frame(clamped);
            drop(world);
            if let Some(t) = tw.upgrade() {
                t.set_current_frame(clamped);
            }
            if let Some(p) = pw.upgrade() {
                p.set_current_frame(clamped);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_add_object_at(move |frame, layer, kind_idx| {
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
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
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_delete_object(move |id| {
            if id < 0 {
                return;
            }
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                world.delete_object(id as usize);
                sync(&t, &world);
            }
        });
    }

    {
        let (state, tw, pw) = (state.clone(), timeline.as_weak(), props_weak.clone());
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
                let world_holder = app_state::active_world(&state);
                let world = world_holder.lock().unwrap();
                crate::ui::properties::select_object(&p, &world, id);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_move_object(move |id, start, layer| {
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                world.move_object(id as usize, start, layer);
                sync(&t, &world);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_resize_object(move |id, start, end| {
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                world.resize_object(id as usize, start, end);
                sync(&t, &world);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_set_zoom(move |scale| {
            let world_holder = app_state::active_world(&state);
            let mut world = world_holder.lock().unwrap();
            world.set_zoom(scale);
            if let Some(t) = tw.upgrade() {
                t.set_zoom_scale(world.zoom());
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_toggle_layer_visible(move |layer| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                let current = world.layer_states();
                let visible = current.get(layer as usize).map(|s| s.0).unwrap_or(true);
                world.set_layer_visible(layer as usize, !visible);
                sync(&t, &world);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_toggle_layer_locked(move |layer| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                let current = world.layer_states();
                let locked = current.get(layer as usize).map(|s| s.1).unwrap_or(false);
                world.set_layer_locked(layer as usize, !locked);
                sync(&t, &world);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_switch_scene_tab(move |id| {
            if let Some(t) = tw.upgrade() {
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                if world.switch_scene(id) {
                    sync(&t, &world);
                    sync_scene_tabs(&t, &world);
                }
            }
        });
    }

    {
        let (state, sw) = (state.clone(), scene_settings_weak.clone());
        timeline.on_open_scene_settings_create(move || {
            if let Some(w) = sw.upgrade() {
                crate::ui::scene_settings::open_for_create(&w, &state);
            }
        });
    }

    {
        let (state, sw) = (state.clone(), scene_settings_weak.clone());
        timeline.on_open_scene_settings_edit(move |scene_id| {
            if let Some(w) = sw.upgrade() {
                crate::ui::scene_settings::open_for_edit(&w, &state, scene_id);
            }
        });
    }

    {
        let (state, tw) = (state.clone(), timeline.as_weak());
        timeline.on_close_scene_tab(move |id| {
            if let Some(t) = tw.upgrade() {
                app_state::snapshot_before_edit(&state);
                let world_holder = app_state::active_world(&state);
                let mut world = world_holder.lock().unwrap();
                if world.scenes().len() > 1 {
                    world.remove_scene(id);
                    sync(&t, &world);
                    sync_scene_tabs(&t, &world);
                }
            }
        });
    }

    sync_active_session(&state, &timeline.as_weak());
}

/// アクティブプロジェクト切替時、タイムライン全体（オブジェクト・レイヤー・シーンタブ）を再同期する。
pub fn sync_active_session(state: &SharedAppState, timeline_weak: &Weak<TimelineWindow>) {
    let Some(t) = timeline_weak.upgrade() else {
        return;
    };
    let world_holder = app_state::active_world(state);
    let world = world_holder.lock().unwrap();
    sync(&t, &world);
    sync_scene_tabs(&t, &world);
    t.set_zoom_scale(world.zoom());
    t.set_layer_count(world.layer_count());
}

fn to_slint(data: &crate::ecs::TimelineData) -> TimelineObject {
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

fn sync_scene_tabs(timeline: &TimelineWindow, world: &EcsWorld) {
    let active = world.active_scene();
    let tabs: Vec<SceneTabItem> = world
        .scenes()
        .iter()
        .map(|s| SceneTabItem {
            id: s.id,
            name: s.name.clone().into(),
            active: s.id == active,
        })
        .collect();
    timeline.set_scene_tabs(ModelRc::new(VecModel::from(tabs)));
}
