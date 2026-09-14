use super::TimelineWindow;
use crate::app_state::{self, SharedAppState};
use crate::ecs::EcsWorld;
use crate::ecs::components::{GroupControl, MediaSource, ShapeParams, TextContent};
use crate::localization::tr;
use crate::objects::registry;
use crate::ui::preview::PreviewPanel;
use std::cell::RefCell;
use std::rc::Rc;

fn trim_object_to_range(world: &mut EcsWorld, id: usize, range_start: i32, range_end: i32) {
    let Some(target) = world
        .get_timeline_objects()
        .iter()
        .find(|o| o.id as usize == id)
        .map(|o| (o.start_frame, o.end_frame))
    else {
        return;
    };
    let (start, end) = target;
    if start >= range_end || end <= range_start {
        world.delete_object(id);
        return;
    }
    let mut current_id = id;
    if start < range_start {
        if let Some(right_id) = world.split_object(current_id, range_start) {
            current_id = right_id;
        }
    }
    if end > range_end {
        if let Some(right_id) = world.split_object(current_id, range_end) {
            world.delete_object(right_id);
        }
    }
}

fn remove_range_content(world: &mut EcsWorld, range_start: i32, range_end: i32) {
    let overlapping: Vec<usize> = world
        .get_timeline_objects()
        .iter()
        .filter(|o| o.start_frame < range_end && o.end_frame > range_start)
        .map(|o| o.id as usize)
        .collect();
    let mut to_delete = Vec::with_capacity(overlapping.len());
    for id in overlapping {
        let Some((start, end)) = world
            .get_timeline_objects()
            .iter()
            .find(|o| o.id as usize == id)
            .map(|o| (o.start_frame, o.end_frame))
        else {
            continue;
        };
        if start >= range_start && end <= range_end {
            to_delete.push(id);
            continue;
        }
        let mut middle_id = id;
        if start < range_start {
            if let Some(right_id) = world.split_object(middle_id, range_start) {
                middle_id = right_id;
            } else {
                continue;
            }
        }
        if end > range_end {
            world.split_object(middle_id, range_end);
        }
        to_delete.push(middle_id);
    }
    if !to_delete.is_empty() {
        world.delete_objects(&to_delete);
    }
}

impl TimelineWindow {
    pub(super) fn after_structural_edit(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn seek(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        frame: i32,
    ) {
        preview_panel.borrow_mut().seek(frame, state);
    }

    pub(super) fn select_object(
        &mut self,
        _state: &SharedAppState,
        _props_weak: &(),
        id: i32,
        additive: bool,
    ) {
        if !additive {
            self.selected_ids.clear();
        }
        self.selected_ids.insert(id);
    }

    pub(super) fn selection_target_ids(&self, hit_id: i32) -> Vec<usize> {
        if self.selected_ids.len() > 1 && self.selected_ids.contains(&hit_id) {
            self.selected_ids.iter().map(|&id| id as usize).collect()
        } else {
            vec![hit_id as usize]
        }
    }

    pub(super) fn add_object_at(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        frame: i32,
        layer: i32,
        kind_idx: i32,
    ) {
        let registry_snapshot = registry();
        let Some(plugin) = registry_snapshot.get(kind_idx as usize) else {
            return;
        };
        let start = frame.max(0);
        let layer = layer.max(0);
        let kind_id = kind_idx as u32;

        match plugin.name.as_str() {
            "Video" | "Image" | "Audio" => {
                let Some(path) = rfd::FileDialog::new().pick_file() else {
                    return;
                };
                let Some(kind) = neoutl_media_runtime::detect_kind(&path) else {
                    return;
                };
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                let media = MediaSource {
                    path,
                    kind,
                    trim_in_frame: 0,
                };
                world.add_media_object(start, 90, kind_id, layer, media);
            }
            "Text" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_object(start, 90, kind_id, layer, Some(TextContent::default()));
            }
            "Shape" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_shape_object(start, 90, kind_id, layer, ShapeParams::default());
            }
            "Scene" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                let host_scene = world.active_scene();
                let default_target = world
                    .scenes()
                    .into_iter()
                    .map(|s| s.id)
                    .find(|&id| !world.would_create_scene_cycle(host_scene, id));
                let Some(default_target) = default_target else {
                    eprintln!(
                        "{}",
                        tr("[NeoUtl] シーンオブジェクト追加を中止: 配置可能なシーンがありません")
                    );
                    return;
                };
                world.add_scene_object(start, 90, kind_id, layer, default_target);
            }
            "Group Control" => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_group_control_object(start, 90, kind_id, layer, GroupControl::default());
            }
            _ => {
                app_state::snapshot_before_edit(state);
                let world_holder = app_state::active_world(state);
                let mut world = world_holder.lock().unwrap();
                world.add_object(start, 90, kind_id, layer, None);
            }
        }
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn delete_objects(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        ids: &[usize],
    ) {
        if ids.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        {
            let world_holder = app_state::active_world(state);
            let mut world = world_holder.lock().unwrap();
            for &id in ids {
                world.delete_object(id);
            }
        }
        for &id in ids {
            self.selected_ids.remove(&(id as i32));
        }
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn split_objects_at(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        ids: &[usize],
        frame: i32,
    ) {
        if ids.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        {
            let world_holder = app_state::active_world(state);
            let mut world = world_holder.lock().unwrap();
            for &id in ids {
                world.split_object(id, frame);
            }
        }
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn keyframe_moved(
        &mut self,
        state: &SharedAppState,
        id: i32,
        old_frame: i32,
        new_frame: i32,
    ) {
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        world_holder
            .lock()
            .unwrap()
            .move_keyframe(id as usize, "", old_frame, new_frame);
    }

    pub(super) fn duplicate_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        hit_id: i32,
    ) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let frame = world.current_frame();
        world.duplicate_objects(&ids, frame, self.selected_layer);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn cut_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        hit_id: i32,
    ) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let docs = world.cut_objects(&ids);
        drop(world);
        app_state::set_clipboard(state, docs);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn copy_requested(&mut self, state: &SharedAppState, hit_id: i32) {
        if hit_id < 0 {
            return;
        }
        let ids = self.selection_target_ids(hit_id);
        let world_holder = app_state::active_world(state);
        let world = world_holder.lock().unwrap();
        let docs = world.copy_objects(&ids);
        drop(world);
        app_state::set_clipboard(state, docs);
    }

    pub(super) fn paste_requested(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        let docs = app_state::clipboard(state);
        if docs.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let frame = world.current_frame();
        world.paste_objects(&docs, frame, self.selected_layer);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn toggle_layer_visible(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        layer: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let visible = world
            .layer_states()
            .get(layer as usize)
            .map_or(true, |s| s.0);
        world.set_layer_visible(layer as usize, !visible);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn toggle_layer_locked(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        layer: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let locked = world
            .layer_states()
            .get(layer as usize)
            .map_or(false, |s| s.1);
        world.set_layer_locked(layer as usize, !locked);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn switch_scene_tab(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
    ) {
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        if world.switch_scene(id) {
            drop(world);
            self.selected_ids.clear();
            preview_panel.borrow_mut().refresh_total_frames(state);
        }
    }

    pub(super) fn cut_selection_range(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        let Some((range_start, range_end)) = self.select_range else {
            return;
        };
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        remove_range_content(&mut world, range_start, range_end);
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn cut_selection_range_and_pack(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
    ) {
        let Some((range_start, range_end)) = self.select_range else {
            return;
        };
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        remove_range_content(&mut world, range_start, range_end);
        let shift = range_end - range_start;
        let followers: Vec<(usize, i32, i32)> = world
            .get_timeline_objects()
            .iter()
            .filter(|o| o.start_frame >= range_end)
            .map(|o| (o.id as usize, o.start_frame, o.layer))
            .collect();
        for (id, start, layer) in followers {
            world.move_object(id, start - shift, layer);
        }
        self.select_range = None;
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn pack_left(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        ids: &[usize],
    ) {
        if ids.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let objects = world.get_timeline_objects();
        for &id in ids {
            let Some(target) = objects.iter().find(|o| o.id as usize == id) else {
                continue;
            };
            let prev_end = objects
                .iter()
                .filter(|o| o.layer == target.layer && o.id as usize != id)
                .filter(|o| o.end_frame <= target.start_frame)
                .map(|o| o.end_frame)
                .max()
                .unwrap_or(0);
            world.move_object(id, prev_end, target.layer);
        }
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn cut_and_pack(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        ids: &[usize],
    ) {
        if ids.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        let removed: Vec<(i32, i32, i32)> = world
            .get_timeline_objects()
            .iter()
            .filter(|o| ids.contains(&(o.id as usize)))
            .map(|o| (o.start_frame, o.end_frame, o.layer))
            .collect();
        let docs = world.cut_objects(ids);
        drop(world);
        app_state::set_clipboard(state, docs);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        for (start, end, layer) in removed {
            let shift = end - start;
            let followers: Vec<(usize, i32)> = world
                .get_timeline_objects()
                .iter()
                .filter(|o| o.layer == layer && o.start_frame >= end)
                .map(|o| (o.id as usize, o.start_frame))
                .collect();
            for (id, follower_start) in followers {
                world.move_object(id, follower_start - shift, layer);
            }
        }
        for &id in ids {
            self.selected_ids.remove(&(id as i32));
        }
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn extract_selection(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        ids: &[usize],
    ) {
        let Some((range_start, range_end)) = self.select_range else {
            return;
        };
        if ids.is_empty() {
            return;
        }
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        for &id in ids {
            trim_object_to_range(&mut world, id, range_start, range_end);
        }
        drop(world);
        preview_panel.borrow_mut().refresh_total_frames(state);
    }

    pub(super) fn close_scene_tab(
        &mut self,
        state: &SharedAppState,
        preview_panel: &Rc<RefCell<PreviewPanel>>,
        id: i32,
    ) {
        app_state::snapshot_before_edit(state);
        let world_holder = app_state::active_world(state);
        let mut world = world_holder.lock().unwrap();
        if world.scenes().len() > 1 {
            if world.remove_scene(id) {
                drop(world);
                preview_panel.borrow_mut().refresh_total_frames(state);
            } else {
                eprintln!(
                    "{}",
                    tr("[NeoUtl] シーン削除を拒否: id={id}（他シーンのSceneObjectから参照中）")
                        .replace("{id}", &id.to_string())
                );
            }
        }
    }
}
