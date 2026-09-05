use crate::ecs::EcsWorld;
use crate::ecs::TimelineData;
use crate::ecs::components::{
    ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource, ObjectId, SceneId,
    TimeRange,
};
use crate::ecs::effects::EffectStack;
use crate::ecs::resources::{LayerStates, SceneResource, TimelineResource};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut};

impl EcsWorld {
    pub fn update_total_frames(&mut self) {
        self.world.run(
            |mut timeline: UniqueViewMut<TimelineResource>, time_ranges: View<TimeRange>| {
                let max_end = time_ranges.iter().map(|t| t.end_frame).max().unwrap_or(0);
                timeline.total_frames = max_end.max(300);
            },
        );
    }

    pub fn set_current_frame(&mut self, frame: i32) {
        self.world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                timeline.current_frame = frame;
            });
        self.touch();
    }

    pub fn current_frame(&self) -> i32 {
        self.world
            .run(|timeline: UniqueView<TimelineResource>| timeline.current_frame)
    }

    pub fn total_frames(&self) -> i32 {
        self.world
            .run(|timeline: UniqueView<TimelineResource>| timeline.total_frames)
    }

    pub fn layer_count(&self) -> i32 {
        self.world
            .run(|timeline: UniqueView<TimelineResource>| timeline.layer_count)
    }

    pub fn set_zoom(&mut self, scale: f32) {
        self.world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                timeline.zoom_scale = scale.clamp(0.1, 10.0);
            });
        self.touch();
    }

    pub fn zoom(&self) -> f32 {
        self.world
            .run(|timeline: UniqueView<TimelineResource>| timeline.zoom_scale)
    }

    pub fn set_layer_visible(&mut self, layer: usize, visible: bool) {
        self.world
            .run(|mut states: UniqueViewMut<LayerStates>| states.set_visible(layer, visible));
        self.touch();
    }

    pub fn set_layer_locked(&mut self, layer: usize, locked: bool) {
        self.world
            .run(|mut states: UniqueViewMut<LayerStates>| states.set_locked(layer, locked));
        self.touch();
    }

    pub fn layer_states(&self) -> Vec<(bool, bool)> {
        self.world
            .run(|states: UniqueView<LayerStates>| states.0.clone())
    }

    pub fn get_timeline_objects(&self) -> Vec<TimelineData> {
        self.world.run(
            |scenes: UniqueView<SceneResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>,
             kind_ids: View<KindId>,
             layers: View<Layer>,
             scene_ids: View<SceneId>,
             media: View<MediaSource>,
             group_controls: View<GroupControl>,
             clip_targets: View<ClipTarget>| {
                let active = scenes.active_scene;
                let mut objs = Vec::new();
                for (_entity, (id, range, kind, layer, scene)) in
                    (&object_ids, &time_ranges, &kind_ids, &layers, &scene_ids)
                        .iter()
                        .with_id()
                {
                    if scene.0 != active {
                        continue;
                    }
                    let (curtain_down, curtain_up) = group_controls
                        .get(_entity)
                        .ok()
                        .map(|gc| (gc.layer_count_down as i32, gc.layer_count_up as i32))
                        .unwrap_or((0, 0));
                    let (clip_down, clip_up) = clip_targets
                        .get(_entity)
                        .ok()
                        .filter(|ct| ct.enabled)
                        .map(|ct| (ct.layer_count_down as i32, ct.layer_count_up as i32))
                        .unwrap_or((0, 0));
                    objs.push(TimelineData {
                        id: id.0 as i32,
                        start_frame: range.start_frame,
                        end_frame: range.end_frame,
                        kind: kind.0 as i32,
                        layer: layer.0,
                        media_path: media.get(_entity).ok().map(|m| m.path.clone()),
                        media_trim_in_frame: media.get(_entity).ok().map_or(0, |m| m.trim_in_frame),
                        group_layer_count_down: curtain_down,
                        group_layer_count_up: curtain_up,
                        clip_layer_count_down: clip_down,
                        clip_layer_count_up: clip_up,
                    });
                }
                objs
            },
        )
    }

    fn snap_to_active_scene(&self, frame: i32) -> i32 {
        self.world.run(|scenes: UniqueView<SceneResource>| {
            scenes
                .find(scenes.active_scene)
                .map_or(frame, |s| s.snap_frame(frame))
        })
    }

    fn snap_magnetic(&self, frame: i32, layer: i32, exclude_id: usize) -> i32 {
        let grid_snapped = self.snap_to_active_scene(frame);
        if grid_snapped != frame {
            return grid_snapped;
        }
        let (range, enabled) = self.world.run(|scenes: UniqueView<SceneResource>| {
            scenes
                .find(scenes.active_scene)
                .map_or((0, false), |s| (s.magnetic_snap_range, s.enable_snap))
        });
        if !enabled || range <= 0 {
            return frame;
        }
        let mut candidates = vec![self.current_frame()];
        self.world.run(
            |scenes: UniqueView<SceneResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>,
             layers: View<Layer>,
             scene_ids: View<SceneId>| {
                let active = scenes.active_scene;
                for (id, r, l, s) in (&object_ids, &time_ranges, &layers, &scene_ids).iter() {
                    if s.0 == active && l.0 == layer && id.0 != exclude_id {
                        candidates.push(r.start_frame);
                        candidates.push(r.end_frame);
                    }
                }
            },
        );
        candidates
            .into_iter()
            .map(|c| (c, (c - frame).abs()))
            .filter(|&(_, d)| d <= range)
            .min_by_key(|&(_, d)| d)
            .map_or(frame, |(c, _)| c)
    }

    fn object_layer(&self, object_id: usize) -> Option<i32> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|layers: View<Layer>| layers.get(entity).ok().map(|l| l.0))
    }

    pub fn object_exists(&self, object_id: usize) -> bool {
        self.find_entity(object_id).is_some()
    }

    pub fn move_object(&mut self, object_id: usize, new_start: i32, new_layer: i32) {
        let new_start = self.snap_magnetic(new_start, new_layer, object_id);
        self.world.run(
            |object_ids: View<ObjectId>,
             mut time_ranges: ViewMut<TimeRange>,
             mut layers: ViewMut<Layer>,
             mut keyframe_tracks: ViewMut<KeyframeTracks>,
             mut effect_stacks: ViewMut<EffectStack>| {
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        let delta = if let Ok(mut range) = (&mut time_ranges).get(entity) {
                            let dur = range.end_frame - range.start_frame;
                            let delta = new_start - range.start_frame;
                            range.start_frame = new_start;
                            range.end_frame = new_start + dur;
                            delta
                        } else {
                            break;
                        };
                        if delta != 0 {
                            if let Ok(mut tracks) = (&mut keyframe_tracks).get(entity) {
                                tracks.shift(delta);
                            }
                            if let Ok(mut stack) = (&mut effect_stacks).get(entity) {
                                for instance in stack.0.iter_mut() {
                                    for param in instance.params.values_mut() {
                                        param.shift_keyframes(delta);
                                    }
                                }
                            }
                        }
                        if let Ok(mut layer) = (&mut layers).get(entity) {
                            layer.0 = new_layer.max(0);
                        }
                        break;
                    }
                }
            },
        );
        self.update_total_frames();
        self.touch();
    }

    pub fn resize_object(&mut self, object_id: usize, new_start: i32, new_end: i32) {
        let layer = self.object_layer(object_id).unwrap_or(0);
        let new_start = self.snap_magnetic(new_start, layer, object_id);
        let new_end = self.snap_magnetic(new_end, layer, object_id);
        self.world.run(
            |object_ids: View<ObjectId>,
             mut time_ranges: ViewMut<TimeRange>,
             mut keyframe_tracks: ViewMut<KeyframeTracks>,
             mut effect_stacks: ViewMut<EffectStack>| {
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        let (old_start, old_end, start, end) =
                            if let Ok(mut range) = (&mut time_ranges).get(entity) {
                                let old_start = range.start_frame;
                                let old_end = range.end_frame;
                                range.start_frame = new_start.max(0);
                                range.end_frame = new_end.max(range.start_frame + 1);
                                (old_start, old_end, range.start_frame, range.end_frame)
                            } else {
                                break;
                            };
                        if let Ok(mut tracks) = (&mut keyframe_tracks).get(entity) {
                            tracks.clamp_to_range(old_start, old_end, start, end);
                        }
                        if let Ok(mut stack) = (&mut effect_stacks).get(entity) {
                            for instance in stack.0.iter_mut() {
                                for param in instance.params.values_mut() {
                                    param.clamp_keyframes_to_range(old_start, old_end, start, end);
                                }
                            }
                        }
                        break;
                    }
                }
            },
        );
        self.update_total_frames();
    }

    pub(crate) fn find_entity(&self, object_id: usize) -> Option<shipyard::EntityId> {
        self.world.run(|object_ids: View<ObjectId>| {
            object_ids
                .iter()
                .with_id()
                .find(|(_, id)| id.0 == object_id)
                .map(|(e, _)| e)
        })
    }

    pub fn ripple_move_object(&mut self, object_id: usize, new_start: i32) {
        let Some(layer) = self.object_layer(object_id) else {
            return;
        };
        let Some(old_start) = self.find_entity(object_id).and_then(|e| {
            self.world
                .run(|time_ranges: View<TimeRange>| time_ranges.get(e).ok().map(|r| r.start_frame))
        }) else {
            return;
        };
        let snapped_start = self.snap_magnetic(new_start, layer, object_id);
        let delta = snapped_start - old_start;
        self.move_object(object_id, snapped_start, layer);
        if delta == 0 {
            return;
        }
        let followers: Vec<(usize, i32)> = self.world.run(
            |object_ids: View<ObjectId>, time_ranges: View<TimeRange>, layers: View<Layer>| {
                (&object_ids, &time_ranges, &layers)
                    .iter()
                    .filter(|(id, r, l)| {
                        id.0 != object_id && l.0 == layer && r.start_frame >= old_start
                    })
                    .map(|(id, r, _)| (id.0, r.start_frame))
                    .collect()
            },
        );
        for (id, start) in followers {
            self.move_object(id, start + delta, layer);
        }
    }

    pub fn ripple_resize_object(&mut self, object_id: usize, new_end: i32) {
        let Some(layer) = self.object_layer(object_id) else {
            return;
        };
        let Some((old_start, old_end)) = self.find_entity(object_id).and_then(|e| {
            self.world.run(|time_ranges: View<TimeRange>| {
                time_ranges
                    .get(e)
                    .ok()
                    .map(|r| (r.start_frame, r.end_frame))
            })
        }) else {
            return;
        };
        let snapped_end = self
            .snap_magnetic(new_end, layer, object_id)
            .max(old_start + 1);
        let delta = snapped_end - old_end;
        self.resize_object(object_id, old_start, snapped_end);
        if delta == 0 {
            return;
        }
        let followers: Vec<(usize, i32)> = self.world.run(
            |object_ids: View<ObjectId>, time_ranges: View<TimeRange>, layers: View<Layer>| {
                (&object_ids, &time_ranges, &layers)
                    .iter()
                    .filter(|(id, r, l)| {
                        id.0 != object_id && l.0 == layer && r.start_frame >= old_end
                    })
                    .map(|(id, r, _)| (id.0, r.start_frame))
                    .collect()
            },
        );
        for (id, start) in followers {
            self.move_object(id, start + delta, layer);
        }
    }
}
