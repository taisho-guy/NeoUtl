pub mod audio_plugins;
pub mod components;
pub mod effects;
pub mod object_schema;
pub mod resources;
pub mod systems;
pub mod transform;
pub mod types;

use crate::document::{DocumentModel, MediaSourceDoc, ObjectDoc, ObjectPayload};

fn resolve_stable_id(kind_id: u32, object_id: usize) -> String {
    match crate::objects::loader::by_kind_id(kind_id) {
        Some(plugin) => plugin.stable_id.clone(),
        None => {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] オブジェクト %{arg0} の kind_id=%{arg1} を stable_id へ解決不能、空値で保存",
                    arg0 = format!("{}", object_id),
                    arg1 = format!("{}", kind_id)
                )
            );
            String::new()
        }
    }
}
use crate::ecs::types::EffectInstance;
use audio_plugins::PluginChain;
use components::{
    AudioParams, ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource, ObjectId,
    ParamAccess, PluginParams, SceneId, SceneObject, ShapeParams, TextContent, TimeRange,
};
use effects::EffectStack;
use resources::{
    LayerStates, ProjectResource, SceneMeta, SceneResource, SystemSettingsResource,
    TimelineResource,
};
use std::collections::HashMap;

use shipyard::{
    AddComponent, Borrow, BorrowInfo, Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut,
    World,
};
use transform::{Camera, GlobalMatrix, Transform, compute_global_matrix};

#[derive(Borrow, BorrowInfo)]
struct ObjectQueryViews<'v> {
    object_ids: View<'v, ObjectId>,
    time_ranges: View<'v, TimeRange>,
    kind_ids: View<'v, KindId>,
    layers: View<'v, Layer>,
    scene_ids: View<'v, SceneId>,
    transforms: View<'v, Transform>,
    audio: View<'v, AudioParams>,
    stacks: View<'v, EffectStack>,
    texts: View<'v, TextContent>,
    shapes: View<'v, ShapeParams>,
    plugins: View<'v, PluginParams>,
    media: View<'v, MediaSource>,
    keyframes: View<'v, KeyframeTracks>,
    plugin_chains: View<'v, PluginChain>,
    scene_objects: View<'v, SceneObject>,
    group_controls: View<'v, GroupControl>,
    clip_targets: View<'v, ClipTarget>,
}

#[derive(Clone, Debug)]
pub struct TimelineData {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub layer: i32,
    pub media_path: Option<std::path::PathBuf>,
    pub media_trim_in_frame: i64,
    pub group_layer_count_down: i32,
    pub group_layer_count_up: i32,
    pub clip_layer_count_down: i32,
    pub clip_layer_count_up: i32,
}

#[derive(Clone, Debug)]
pub struct SceneSettings {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub grid_mode: i32,
    pub grid_bpm: f32,
    pub grid_offset: f32,
    pub grid_interval: i32,
    pub grid_subdivision: i32,
    pub enable_snap: bool,
    pub magnetic_snap_range: i32,
}

impl From<&SceneMeta> for SceneSettings {
    fn from(s: &SceneMeta) -> Self {
        Self {
            name: s.name.clone(),
            width: s.width,
            height: s.height,
            fps: s.fps,
            grid_mode: s.grid_mode,
            grid_bpm: s.grid_bpm,
            grid_offset: s.grid_offset,
            grid_interval: s.grid_interval,
            grid_subdivision: s.grid_subdivision,
            enable_snap: s.enable_snap,
            magnetic_snap_range: s.magnetic_snap_range,
        }
    }
}

pub struct EcsWorld {
    pub world: World,
    selected_ids: std::collections::HashSet<usize>,
    revision: u64,
}

impl EcsWorld {
    pub fn new() -> Self {
        let world = World::new();
        world.add_unique(TimelineResource::new());
        world.add_unique(ProjectResource::new());
        world.add_unique(LayerStates::new(resources::DEFAULT_LAYER_COUNT));
        world.add_unique(SceneResource::new());
        world.add_unique(SystemSettingsResource::new());
        world.add_unique(Camera::default());
        Self {
            world,
            selected_ids: std::collections::HashSet::new(),
            revision: 0,
        }
    }

    fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_selected_ids(&mut self, ids: std::collections::HashSet<usize>) {
        self.selected_ids = ids;
    }

    pub fn is_selected(&self, id: usize) -> bool {
        self.selected_ids.contains(&id)
    }

    pub fn add_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        text: Option<TextContent>,
    ) -> usize {
        let (id, scene_id) = self.world.run(
            |mut timeline: UniqueViewMut<TimelineResource>, scenes: UniqueView<SceneResource>| {
                let id = timeline.next_id;
                timeline.next_id += 1;
                (id, scenes.active_scene)
            },
        );

        let entity = self.world.add_entity((
            ObjectId(id),
            TimeRange {
                start_frame: start,
                end_frame: start + duration,
            },
            KindId(kind_id),
            Layer(layer),
            SceneId(scene_id),
            Transform::default(),
            GlobalMatrix::default(),
            EffectStack::default(),
        ));

        let is_audio_kind = crate::objects::loader::by_kind_id(kind_id)
            .is_some_and(|p| p.stable_id == neoutl_object_api::AUDIO_STABLE_ID);
        if is_audio_kind {
            self.world.add_component(entity, AudioParams::default());
        }

        if let Some(t) = text {
            self.world.add_component(entity, t);
        }

        self.update_total_frames();
        self.touch();
        id
    }

    pub fn add_shape_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        shape: ShapeParams,
    ) -> usize {
        let id = self.add_object(start, duration, kind_id, layer, None);
        if let Some(entity) = self.find_entity(id) {
            self.world.add_component(entity, shape);
        }
        self.touch();
        id
    }

    pub fn add_media_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        media: MediaSource,
    ) -> usize {
        let id = self.add_object(start, duration, kind_id, layer, None);
        if let Some(entity) = self.find_entity(id) {
            self.world.add_component(entity, media);
        }
        self.touch();
        id
    }

    pub fn add_scene_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        target_scene: i32,
    ) -> usize {
        let id = self.add_object(start, duration, kind_id, layer, None);
        if let Some(entity) = self.find_entity(id) {
            self.world
                .add_component(entity, SceneObject { target_scene });
        }
        self.touch();
        id
    }

    pub fn add_group_control_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        gc: GroupControl,
    ) -> usize {
        let id = self.add_object(start, duration, kind_id, layer, None);
        if let Some(entity) = self.find_entity(id) {
            self.world.add_component(entity, gc);
        }
        self.touch();
        id
    }

    pub fn set_group_control(&mut self, object_id: usize, gc: GroupControl) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut controls: ViewMut<GroupControl>| {
            if let Ok(mut slot) = (&mut controls).get(entity) {
                *slot = gc;
            }
        });
        self.touch();
    }

    pub fn set_clip_target(&mut self, object_id: usize, ct: ClipTarget) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        if self
            .world
            .run(|targets: View<ClipTarget>| targets.get(entity).is_ok())
        {
            self.world.run(|mut targets: ViewMut<ClipTarget>| {
                if let Ok(mut slot) = (&mut targets).get(entity) {
                    *slot = ct;
                }
            });
        } else {
            self.world.add_component(entity, ct);
        }
        self.touch();
    }

    pub fn get_clip_target(&self, object_id: usize) -> ClipTarget {
        let Some(entity) = self.find_entity(object_id) else {
            return ClipTarget::default();
        };
        self.world
            .run(|targets: View<ClipTarget>| targets.get(entity).copied().unwrap_or_default())
    }

    #[cfg(test)]
    pub fn set_layer(&mut self, object_id: usize, layer: i32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut layers: ViewMut<Layer>| {
            if let Ok(mut slot) = (&mut layers).get(entity) {
                *slot = Layer(layer);
            }
        });
    }

    #[cfg(test)]
    pub fn set_transform_param(&mut self, object_id: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(
            |mut transforms: ViewMut<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                if let Ok(mut slot) = (&mut transforms).get(entity) {
                    slot.set_param(key, value);
                    if let Ok(mut matrix) = (&mut matrices).get(entity) {
                        *matrix = compute_global_matrix(&slot);
                    }
                }
            },
        );
    }

    pub fn delete_object(&mut self, id: usize) {
        let mut target_entity = None;
        self.world.run(|object_ids: View<ObjectId>| {
            for (entity, obj_id) in object_ids.iter().with_id() {
                if obj_id.0 == id {
                    target_entity = Some(entity);
                    break;
                }
            }
        });

        if let Some(entity) = target_entity {
            self.world.delete_entity(entity);
            self.update_total_frames();
        }
        self.touch();
    }

    pub fn delete_objects(&mut self, ids: &[usize]) {
        for &id in ids {
            let mut target_entity = None;
            self.world.run(|object_ids: View<ObjectId>| {
                for (entity, obj_id) in object_ids.iter().with_id() {
                    if obj_id.0 == id {
                        target_entity = Some(entity);
                        break;
                    }
                }
            });
            if let Some(entity) = target_entity {
                self.world.delete_entity(entity);
            }
        }
        self.update_total_frames();
        self.touch();
    }

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

    pub fn set_fps(&mut self, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.fps = fps;
            });
        self.touch();
    }

    pub fn set_resolution(&mut self, width: u32, height: u32) {
        let fps = self.get_project().fps;
        self.apply_scene_resolution(width, height, fps);
    }

    pub fn get_project(&self) -> ProjectResource {
        self.world
            .run(|project: UniqueView<ProjectResource>| project.clone())
    }

    pub fn set_project_meta(&mut self, name: String, dir: std::path::PathBuf) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.name = name;
                project.dir = Some(dir);
            });
        self.touch();
    }

    pub fn set_audio_format(&mut self, sample_rate: u32, channels: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.audio_sample_rate = sample_rate;
                project.audio_channels = channels;
            });
        self.touch();
    }

    fn apply_scene_resolution(&mut self, width: u32, height: u32, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.width = width;
                project.height = height;
                project.fps = fps;
            });
        self.set_camera(Camera::for_resolution(width as f32, height as f32));
        self.touch();
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

    fn find_entity(&self, object_id: usize) -> Option<shipyard::EntityId> {
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

    fn spawn_object_from_doc(&mut self, o: &ObjectDoc) -> shipyard::EntityId {
        let kind_id = match crate::objects::loader::by_stable_id(&o.kind_stable_id) {
            Some(plugin) => plugin.kind_id,
            None => {
                eprintln!(
                    "{}",
                    t!(
                        "[NeoUtl] オブジェクト %{arg0} のプラグイン未検出、無描画で保持: stable_id=%{arg1}",
                        arg0 = format!("{}", o.id),
                        arg1 = format!("{}", o.kind_stable_id)
                    )
                );
                crate::objects::loader::UNRESOLVED_KIND_ID
            }
        };
        let is_audio_kind = o.kind_stable_id == neoutl_object_api::AUDIO_STABLE_ID;
        let entity = self.world.add_entity((
            ObjectId(o.id),
            TimeRange {
                start_frame: o.start_frame,
                end_frame: o.end_frame,
            },
            KindId(kind_id),
            Layer(o.layer),
            SceneId(o.scene_id),
            o.transform,
            GlobalMatrix::default(),
            EffectStack(o.effects.clone()),
        ));
        if is_audio_kind {
            self.world.add_component(entity, o.audio);
        }
        if let Some(t) = &o.payload.text {
            self.world.add_component(entity, t.clone());
        }
        if let Some(s) = &o.payload.shape {
            self.world.add_component(entity, *s);
        }
        if let Some(p) = &o.payload.plugin_params {
            self.world.add_component(entity, PluginParams(p.clone()));
        }
        if let Some(chain) = &o.payload.plugin_chain {
            let mut chain = PluginChain(chain.clone());
            chain.repair_instance_uids();
            self.world.add_component(entity, chain);
        }
        if let Some(m) = &o.payload.media {
            self.world.add_component(entity, MediaSource::from(m));
        }
        if let Some(target_scene) = o.payload.scene {
            self.world
                .add_component(entity, SceneObject { target_scene });
        }
        if let Some(gc) = o.payload.group_control {
            self.world.add_component(entity, gc);
        }
        if let Some(ct) = o.payload.clip_target {
            self.world.add_component(entity, ct);
        }
        if !o.keyframes.is_empty() {
            self.world
                .add_component(entity, KeyframeTracks(o.keyframes.clone()));
        }
        entity
    }

    fn alloc_object_id(&mut self) -> usize {
        self.world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                let id = timeline.next_id;
                timeline.next_id += 1;
                id
            })
    }

    pub fn copy_objects(&self, ids: &[usize]) -> Vec<ObjectDoc> {
        self.world.run(|views: ObjectQueryViews| {
            let mut docs = Vec::new();
            for (entity, (id, range, kind, layer, scene)) in (
                &views.object_ids,
                &views.time_ranges,
                &views.kind_ids,
                &views.layers,
                &views.scene_ids,
            )
                .iter()
                .with_id()
            {
                if !ids.contains(&id.0) {
                    continue;
                }
                docs.push(ObjectDoc {
                    id: id.0,
                    scene_id: scene.0,
                    kind_stable_id: resolve_stable_id(kind.0, id.0),
                    layer: layer.0,
                    start_frame: range.start_frame,
                    end_frame: range.end_frame,
                    transform: views.transforms.get(entity).copied().unwrap_or_default(),
                    audio: views.audio.get(entity).copied().unwrap_or_default(),
                    keyframes: views
                        .keyframes
                        .get(entity)
                        .map(|k| k.0.clone())
                        .unwrap_or_default(),
                    effects: views
                        .stacks
                        .get(entity)
                        .map(|s| s.0.clone())
                        .unwrap_or_default(),
                    payload: ObjectPayload {
                        text: views.texts.get(entity).ok().cloned(),
                        shape: views.shapes.get(entity).ok().copied(),
                        plugin_params: views.plugins.get(entity).ok().map(|p| p.0.clone()),
                        plugin_chain: views.plugin_chains.get(entity).ok().map(|c| c.0.clone()),
                        media: views.media.get(entity).ok().map(MediaSourceDoc::from),
                        scene: views.scene_objects.get(entity).ok().map(|s| s.target_scene),
                        group_control: views.group_controls.get(entity).ok().copied(),
                        clip_target: views.clip_targets.get(entity).ok().copied(),
                    },
                });
            }
            docs
        })
    }

    pub fn paste_objects(
        &mut self,
        docs: &[ObjectDoc],
        target_frame: i32,
        target_layer: i32,
    ) -> Vec<usize> {
        if docs.is_empty() {
            return Vec::new();
        }
        let anchor_start = docs.iter().map(|d| d.start_frame).min().unwrap_or(0);
        let anchor_layer = docs.iter().map(|d| d.layer).min().unwrap_or(0);
        let active_scene = self.active_scene();
        let mut new_ids = Vec::with_capacity(docs.len());
        for d in docs {
            let dur = d.end_frame - d.start_frame;
            let new_start = (target_frame + (d.start_frame - anchor_start)).max(0);
            let new_layer = (target_layer + (d.layer - anchor_layer)).max(0);
            let new_id = self.alloc_object_id();
            let mut doc = d.clone();
            doc.id = new_id;
            doc.scene_id = active_scene;
            doc.start_frame = new_start;
            doc.end_frame = new_start + dur;
            doc.layer = new_layer;
            self.spawn_object_from_doc(&doc);
            new_ids.push(new_id);
        }
        self.recompute_global_matrices();
        self.update_total_frames();
        new_ids
    }

    pub fn duplicate_objects(
        &mut self,
        ids: &[usize],
        target_frame: i32,
        target_layer: i32,
    ) -> Vec<usize> {
        let docs = self.copy_objects(ids);
        self.paste_objects(&docs, target_frame, target_layer)
    }

    pub fn cut_objects(&mut self, ids: &[usize]) -> Vec<ObjectDoc> {
        let docs = self.copy_objects(ids);
        self.delete_objects(ids);
        docs
    }

    pub fn split_object(&mut self, object_id: usize, split_frame: i32) -> Option<usize> {
        let entity = self.find_entity(object_id)?;

        let snapshot = self.world.run(|v: ObjectQueryViews| {
            let range = v.time_ranges.get(entity).ok().copied()?;
            if split_frame <= range.start_frame || split_frame >= range.end_frame {
                return None;
            }
            Some((
                range,
                v.kind_ids.get(entity).ok().copied()?,
                v.layers.get(entity).ok().copied()?,
                v.scene_ids.get(entity).ok().copied()?,
                v.transforms.get(entity).ok().copied().unwrap_or_default(),
                v.audio.get(entity).ok().copied().unwrap_or_default(),
                v.stacks.get(entity).ok().cloned().unwrap_or_default(),
                v.texts.get(entity).ok().cloned(),
                v.shapes.get(entity).ok().copied(),
                v.plugins.get(entity).ok().cloned(),
                v.media.get(entity).ok().cloned(),
                v.keyframes.get(entity).ok().cloned(),
            ))
        })?;

        let (
            range,
            kind,
            layer,
            scene,
            transform,
            audio,
            mut stack_first,
            text,
            shape,
            plugins,
            media,
            keyframes,
        ) = snapshot;

        let stack_second = stack_first.split_at(split_frame);

        let (keyframes_first, keyframes_second, evaluated) = match keyframes {
            Some(mut kt) => {
                let fallback_for = |key: &str| -> Option<f32> {
                    transform
                        .get_param(key)
                        .or_else(|| audio.get_param(key))
                        .or_else(|| text.as_ref().and_then(|t| t.get_param(key)))
                        .or_else(|| shape.and_then(|s| s.get_param(key)))
                };
                let (second, evaluated) = kt.split_at(split_frame, fallback_for);
                (Some(kt), Some(second), evaluated)
            }
            None => (None, None, HashMap::new()),
        };

        let mut transform2 = transform;
        let mut audio2 = audio;
        let mut text2 = text;
        let mut shape2 = shape;
        for (key, value) in &evaluated {
            if transform2.set_param(key, *value) {
                continue;
            }
            if audio2.set_param(key, *value) {
                continue;
            }
            if let Some(t) = text2.as_mut() {
                if t.set_param(key, *value) {
                    continue;
                }
            }
            if let Some(s) = shape2.as_mut() {
                s.set_param(key, *value);
            }
        }

        self.world.run(
            |mut time_ranges: ViewMut<TimeRange>, mut stacks: ViewMut<EffectStack>| {
                if let Ok(mut r) = (&mut time_ranges).get(entity) {
                    r.end_frame = split_frame;
                }
                if let Ok(mut s) = (&mut stacks).get(entity) {
                    *s = stack_first;
                }
            },
        );
        if let Some(kf) = keyframes_first {
            self.world.add_component(entity, kf);
        }

        let new_id = self
            .world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                let id = timeline.next_id;
                timeline.next_id += 1;
                id
            });

        let new_entity = self.world.add_entity((
            ObjectId(new_id),
            TimeRange {
                start_frame: split_frame,
                end_frame: range.end_frame,
            },
            kind,
            layer,
            scene,
            transform2,
            GlobalMatrix::default(),
            audio2,
            stack_second,
        ));

        if let Some(t) = text2 {
            self.world.add_component(new_entity, t);
        }
        if let Some(s) = shape2 {
            self.world.add_component(new_entity, s);
        }
        if let Some(p) = plugins {
            self.world.add_component(new_entity, p);
        }
        if let Some(mut m) = media {
            m.trim_in_frame += (split_frame - range.start_frame) as i64;
            self.world.add_component(new_entity, m);
        }
        if let Some(kf) = keyframes_second.filter(|kt| !kt.0.is_empty()) {
            self.world.add_component(new_entity, kf);
        }

        self.update_total_frames();
        Some(new_id)
    }

    pub fn get_transform(&self, object_id: usize) -> Option<Transform> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|transforms: View<Transform>| transforms.get(entity).ok().copied())
    }

    pub fn set_transform(&mut self, object_id: usize, t: Transform) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(
            |mut transforms: ViewMut<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                if let Ok(mut slot) = (&mut transforms).get(entity) {
                    *slot = t;
                }
                if let Ok(mut matrix) = (&mut matrices).get(entity) {
                    *matrix = compute_global_matrix(&t);
                }
            },
        );
        self.touch();
    }

    pub fn recompute_global_matrices(&mut self) {
        self.world.run(
            |transforms: View<Transform>, mut matrices: ViewMut<GlobalMatrix>| {
                for (entity, t) in transforms.iter().with_id() {
                    if let Ok(mut matrix) = (&mut matrices).get(entity) {
                        *matrix = compute_global_matrix(t);
                    }
                }
            },
        );
    }

    pub fn set_camera(&mut self, camera: Camera) {
        self.world
            .run(|mut slot: UniqueViewMut<Camera>| *slot = camera);
    }

    pub fn add_effect(&mut self, object_id: usize, effect_id: &str) {
        if effects::find_effect(effect_id).is_none() {
            return;
        }
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.push(effect_id);
            }
        });
        self.touch();
    }

    pub fn reorder_effect(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity)
                && from < stack.0.len()
                && to < stack.0.len()
            {
                let item = stack.0.remove(from);
                stack.0.insert(to, item);
            }
        });
    }

    pub fn set_effect_enabled(&mut self, object_id: usize, index: usize, enabled: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_enabled(index, enabled);
            }
        });
        self.touch();
    }

    pub fn remove_effect(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.remove(index);
            }
        });
        self.touch();
    }

    pub fn set_effect_param(&mut self, object_id: usize, index: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_f32(index, key, value);
            }
        });
        self.touch();
    }

    pub fn get_plugin_chain(
        &self,
        object_id: usize,
    ) -> Option<Vec<audio_plugins::PluginInstanceRef>> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|chains: View<PluginChain>| chains.get(entity).ok().map(|c| c.0.clone()))
    }

    pub fn add_audio_plugin(
        &mut self,
        object_id: usize,
        entry: &maolan_host_adapter::PluginCatalogEntry,
        param_info: Vec<maolan_host_adapter::PluginParamInfo>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            let instance = audio_plugins::PluginInstanceRef {
                instance_uid: 0,
                format: entry.format,
                path: entry.path.clone(),
                plugin_id: entry.plugin_id.clone(),
                bypass: false,
                params: HashMap::new(),
                param_info,
            };
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.0.push(instance);
                chain.repair_instance_uids();
            } else {
                let mut chain = PluginChain(vec![instance]);
                chain.repair_instance_uids();
                chains.add_component_unchecked(entity, chain);
            }
        });
    }

    pub fn remove_audio_plugin(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if index < chain.0.len() {
                    chain.0.remove(index);
                }
            }
        });
    }

    pub fn set_audio_plugin_bypass(&mut self, object_id: usize, index: usize, bypass: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if let Some(inst) = chain.0.get_mut(index) {
                    inst.bypass = bypass;
                }
            }
        });
    }

    pub fn reorder_audio_plugin(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if from < chain.0.len() && to < chain.0.len() {
                    let item = chain.0.remove(from);
                    chain.0.insert(to, item);
                }
            }
        });
    }

    pub fn set_audio_plugin_param(
        &mut self,
        object_id: usize,
        index: usize,
        param_id: u32,
        value: f64,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                if let Some(inst) = chain.0.get_mut(index) {
                    inst.params.insert(param_id, value);
                }
            }
        });
    }

    pub fn set_effect_param_bool(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: bool,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_bool(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_text(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: String,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_text(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_path(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: String,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_path(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_enum(&mut self, object_id: usize, index: usize, key: &str, value: u32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_enum(index, key, value);
            }
        });
        self.touch();
    }

    pub fn set_effect_param_track_ref(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        value: i32,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_param_track_ref(index, key, value);
            }
        });
        self.touch();
    }

    pub fn get_effects(&self, object_id: usize) -> Vec<EffectInstance> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks.get(entity).map(|s| s.0.clone()).unwrap_or_default()
        })
    }

    pub fn get_text(&self, object_id: usize) -> Option<TextContent> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|texts: View<TextContent>| texts.get(entity).ok().cloned())
    }

    pub fn set_text(&mut self, object_id: usize, text: String, font_size: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                slot.text = text;
                slot.font_size = font_size;
            }
        });
        self.touch();
    }

    pub fn set_text_param(&mut self, object_id: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                ParamAccess::set_param(&mut *slot, key, value);
            }
        });
        self.touch();
    }

    pub fn set_text_font_stack(&mut self, object_id: usize, stack: Vec<String>) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                slot.font_family_stack = if stack.is_empty() {
                    vec![String::new()]
                } else {
                    stack
                };
            }
        });
        self.touch();
    }

    pub fn get_shape(&self, object_id: usize) -> Option<ShapeParams> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|shapes: View<ShapeParams>| shapes.get(entity).ok().copied())
    }

    pub fn set_audio_params(&mut self, object_id: usize, volume: f32, pan: f32, mute: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut audio: ViewMut<AudioParams>| {
            if let Ok(mut slot) = (&mut audio).get(entity) {
                slot.volume = volume;
                slot.pan = pan;
                slot.mute = mute;
            }
        });
    }

    pub fn get_audio_params(&self, object_id: usize) -> Option<AudioParams> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|audio: View<AudioParams>| audio.get(entity).ok().copied())
    }

    pub fn is_audio_object(&self, object_id: usize) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|kind_ids: View<KindId>| {
            kind_ids
                .get(entity)
                .ok()
                .and_then(|k| crate::objects::loader::by_kind_id(k.0))
                .is_some_and(|p| p.stable_id == neoutl_object_api::AUDIO_STABLE_ID)
        })
    }

    pub fn get_group_control(&self, object_id: usize) -> Option<GroupControl> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|controls: View<GroupControl>| controls.get(entity).ok().copied())
    }

    pub fn set_keyframe(
        &mut self,
        object_id: usize,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        let mut tracks = self
            .world
            .run(|t: View<KeyframeTracks>| t.get(entity).ok().cloned())
            .unwrap_or_default();
        tracks.set_keyframe(key, frame, value, engine_id, engine_payload);
        self.world.add_component(entity, tracks);
        self.touch();
    }

    pub fn remove_keyframe(&mut self, object_id: usize, key: &str, frame: i32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut tracks: ViewMut<KeyframeTracks>| {
            if let Ok(mut t) = (&mut tracks).get(entity) {
                t.remove_keyframe(key, frame);
            }
        });
        self.touch();
    }

    pub fn move_keyframe(
        &mut self,
        object_id: usize,
        key: &str,
        old_frame: i32,
        new_frame: i32,
    ) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|mut tracks: ViewMut<KeyframeTracks>| {
            (&mut tracks)
                .get(entity)
                .ok()
                .map(|mut t| t.move_keyframe(key, old_frame, new_frame))
                .unwrap_or(false)
        })
    }

    pub fn get_keyframes(&self, object_id: usize, key: &str) -> Vec<crate::ecs::types::Keyframe> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|t: View<KeyframeTracks>| {
            t.get(entity)
                .ok()
                .and_then(|tracks| tracks.0.get(key).cloned())
                .unwrap_or_default()
        })
    }

    pub fn set_effect_keyframe(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        frame: i32,
        value: f32,
        engine_id: String,
        engine_payload: Vec<u8>,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.set_keyframe(index, key, frame, value, engine_id, engine_payload);
            }
        });
        self.touch();
    }

    pub fn remove_effect_keyframe(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        frame: i32,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.remove_keyframe(index, key, frame);
            }
        });
        self.touch();
    }

    pub fn get_effect_keyframes(
        &self,
        object_id: usize,
        index: usize,
        key: &str,
    ) -> Vec<crate::ecs::types::Keyframe> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks
                .get(entity)
                .ok()
                .and_then(|s| s.0.get(index))
                .and_then(|e| e.params.get(key))
                .map(|p| p.keyframes.clone())
                .unwrap_or_default()
        })
    }

    pub fn add_scene(&mut self, name: impl Into<String>) -> i32 {
        let project = self.get_project();
        self.world.run(|mut scenes: UniqueViewMut<SceneResource>| {
            let id = scenes.next_scene_id;
            scenes.next_scene_id += 1;
            let mut meta = SceneMeta::new(id, name);
            meta.width = project.width;
            meta.height = project.height;
            meta.fps = project.fps;
            scenes.scenes.push(meta);
            id
        })
    }

    fn scene_edges(&self) -> HashMap<i32, Vec<i32>> {
        self.world.run(
            |scene_ids: View<SceneId>, scene_objects: View<SceneObject>| {
                let mut edges: HashMap<i32, Vec<i32>> = HashMap::new();
                for (entity, obj) in scene_objects.iter().with_id() {
                    if let Ok(scene) = scene_ids.get(entity) {
                        edges.entry(scene.0).or_default().push(obj.target_scene);
                    }
                }
                edges
            },
        )
    }

    pub fn would_create_scene_cycle(&self, from_scene: i32, target_scene: i32) -> bool {
        if from_scene == target_scene {
            return true;
        }
        let edges = self.scene_edges();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![target_scene];
        while let Some(cur) = stack.pop() {
            if cur == from_scene {
                return true;
            }
            if !visited.insert(cur) {
                continue;
            }
            if let Some(next) = edges.get(&cur) {
                stack.extend(next.iter().copied());
            }
        }
        false
    }

    pub fn scenes_referencing(&self, scene_id: i32) -> Vec<i32> {
        self.world.run(
            |scene_ids: View<SceneId>, scene_objects: View<SceneObject>| {
                let mut referrers = Vec::new();
                for (entity, obj) in scene_objects.iter().with_id() {
                    if obj.target_scene == scene_id {
                        if let Ok(scene) = scene_ids.get(entity) {
                            referrers.push(scene.0);
                        }
                    }
                }
                referrers
            },
        )
    }

    pub fn remove_scene(&mut self, scene_id: i32) -> bool {
        if !self.scenes_referencing(scene_id).is_empty() {
            return false;
        }
        let mut removed_entities = Vec::new();
        self.world
            .run(|object_ids: View<ObjectId>, scene_ids: View<SceneId>| {
                for (entity, (_, s)) in (&object_ids, &scene_ids).iter().with_id() {
                    if s.0 == scene_id {
                        removed_entities.push(entity);
                    }
                }
            });
        for entity in removed_entities {
            self.world.delete_entity(entity);
        }
        self.world.run(|mut scenes: UniqueViewMut<SceneResource>| {
            scenes.scenes.retain(|s| s.id != scene_id);
            if scenes.active_scene == scene_id {
                scenes.active_scene = scenes.scenes.first().map_or(0, |s| s.id);
            }
        });
        true
    }

    pub fn switch_scene(&mut self, scene_id: i32) -> bool {
        let current_states = self.layer_states();
        let switched = self
            .world
            .run(|mut scenes: UniqueViewMut<SceneResource>| -> bool {
                if scenes.find(scene_id).is_none() {
                    return false;
                }
                let active = scenes.active_scene;
                if let Some(prev) = scenes.find_mut(active) {
                    prev.layer_states.clone_from(&current_states);
                }
                scenes.active_scene = scene_id;
                true
            });
        if switched {
            let (total_frames, target_states, width, height, fps) = self.world.run(
                |scenes: UniqueView<SceneResource>| -> (i32, Vec<(bool, bool)>, u32, u32, u32) {
                    let scene = scenes.find(scene_id).expect("checked above");
                    (
                        scene.total_frames,
                        scene.layer_states.clone(),
                        scene.width,
                        scene.height,
                        scene.fps,
                    )
                },
            );
            self.world
                .run(|mut states: UniqueViewMut<LayerStates>| states.0 = target_states);
            self.world
                .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                    timeline.total_frames = total_frames;
                });
            self.apply_scene_resolution(width, height, fps);
        }
        switched
    }

    pub fn active_scene(&self) -> i32 {
        self.world
            .run(|scenes: UniqueView<SceneResource>| scenes.active_scene)
    }

    pub fn scenes(&self) -> Vec<SceneMeta> {
        self.world
            .run(|scenes: UniqueView<SceneResource>| scenes.scenes.clone())
    }

    pub fn get_scene(&self, scene_id: i32) -> Option<SceneSettings> {
        self.world
            .run(|scenes: UniqueView<SceneResource>| scenes.find(scene_id).map(SceneSettings::from))
    }

    pub fn update_scene_settings(&mut self, scene_id: i32, s: SceneSettings) -> bool {
        let updated = self
            .world
            .run(|mut scenes: UniqueViewMut<SceneResource>| -> bool {
                let Some(meta) = scenes.find_mut(scene_id) else {
                    return false;
                };
                meta.name.clone_from(&s.name);
                meta.width = s.width;
                meta.height = s.height;
                meta.fps = s.fps;
                meta.grid_mode = s.grid_mode;
                meta.grid_bpm = s.grid_bpm;
                meta.grid_offset = s.grid_offset;
                meta.grid_interval = s.grid_interval;
                meta.grid_subdivision = s.grid_subdivision;
                meta.enable_snap = s.enable_snap;
                meta.magnetic_snap_range = s.magnetic_snap_range;
                true
            });
        if updated && self.active_scene() == scene_id {
            self.apply_scene_resolution(s.width, s.height, s.fps);
        }
        updated
    }

    pub fn get_system_settings(&self) -> SystemSettingsResource {
        self.world
            .run(|s: UniqueView<SystemSettingsResource>| s.clone())
    }

    pub fn set_system_settings(&mut self, s: SystemSettingsResource) {
        self.world
            .run(|mut slot: UniqueViewMut<SystemSettingsResource>| *slot = s);
    }

    pub fn to_document(&self) -> DocumentModel {
        let project = self.get_project();
        let active_scene = self.active_scene();
        let scenes = self.scenes();
        let next_object_id = self.world.run(|t: UniqueView<TimelineResource>| t.next_id);

        let objects = self.world.run(|views: ObjectQueryViews| {
            let mut objs = Vec::new();
            for (entity, (id, range, kind, layer, scene)) in (
                &views.object_ids,
                &views.time_ranges,
                &views.kind_ids,
                &views.layers,
                &views.scene_ids,
            )
                .iter()
                .with_id()
            {
                objs.push(ObjectDoc {
                    id: id.0,
                    scene_id: scene.0,
                    kind_stable_id: resolve_stable_id(kind.0, id.0),
                    layer: layer.0,
                    start_frame: range.start_frame,
                    end_frame: range.end_frame,
                    transform: views.transforms.get(entity).copied().unwrap_or_default(),
                    audio: views.audio.get(entity).copied().unwrap_or_default(),
                    keyframes: views
                        .keyframes
                        .get(entity)
                        .map(|k| k.0.clone())
                        .unwrap_or_default(),
                    effects: views
                        .stacks
                        .get(entity)
                        .map(|s| s.0.clone())
                        .unwrap_or_default(),
                    payload: ObjectPayload {
                        text: views.texts.get(entity).ok().cloned(),
                        shape: views.shapes.get(entity).ok().copied(),
                        plugin_params: views.plugins.get(entity).ok().map(|p| p.0.clone()),
                        plugin_chain: views.plugin_chains.get(entity).ok().map(|c| c.0.clone()),
                        media: views.media.get(entity).ok().map(MediaSourceDoc::from),
                        scene: views.scene_objects.get(entity).ok().map(|s| s.target_scene),
                        group_control: views.group_controls.get(entity).ok().copied(),
                        clip_target: views.clip_targets.get(entity).ok().copied(),
                    },
                });
            }
            objs
        });

        DocumentModel {
            project_name: project.name,
            audio_sample_rate: project.audio_sample_rate,
            audio_channels: project.audio_channels,
            active_scene,
            next_object_id,
            scenes,
            objects,
        }
    }

    pub fn load_document(&mut self, doc: &DocumentModel) {
        let all: Vec<shipyard::EntityId> = self
            .world
            .run(|ids: View<ObjectId>| ids.iter().with_id().map(|(e, _)| e).collect());
        for e in all {
            self.world.delete_entity(e);
        }

        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.name.clone_from(&doc.project_name);
                project.audio_sample_rate = doc.audio_sample_rate;
                project.audio_channels = doc.audio_channels;
            });
        self.world.run(|mut scenes: UniqueViewMut<SceneResource>| {
            let next_scene_id = doc.scenes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            scenes.scenes.clone_from(&doc.scenes);
            scenes.active_scene = doc.active_scene;
            scenes.next_scene_id = next_scene_id;
        });
        self.world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                timeline.next_id = doc.next_object_id;
            });

        for o in &doc.objects {
            self.spawn_object_from_doc(o);
        }

        self.recompute_global_matrices();
        if let Some(scene) = doc.scenes.iter().find(|s| s.id == doc.active_scene) {
            self.apply_scene_resolution(scene.width, scene.height, scene.fps);
        }
        self.update_total_frames();
        self.touch();
    }
}
