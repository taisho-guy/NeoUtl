// src/ecs/mod.rs
pub mod components;
pub mod effects;
pub mod resources;
pub mod systems;
pub mod transform;
pub mod types;

use crate::ecs::types::EffectInstance;
use components::{AudioParams, KindId, Layer, ObjectId, SceneId, TextContent, TimeRange};
use effects::EffectStack;
use resources::{
    LayerStates, ProjectResource, SceneMeta, SceneResource, SystemSettingsResource,
    TimelineResource,
};

use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut, World};
use transform::{GlobalMatrix, Transform, compute_global_matrix};

/// タイムラインUIに渡すオブジェクト情報（Slint型に非依存）
#[derive(Clone, Debug)]
pub struct TimelineData {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub layer: i32,
}

/// シーン設定ウィンドウとの受け渡し用（AviQtl::UI::SceneData の設定サブセットに相当）
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
}

impl EcsWorld {
    pub fn new() -> Self {
        let world = World::new();
        world.add_unique(TimelineResource::new());
        world.add_unique(ProjectResource::new());
        world.add_unique(LayerStates::new(resources::DEFAULT_LAYER_COUNT));
        world.add_unique(SceneResource::new());
        world.add_unique(SystemSettingsResource::new());
        Self { world }
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
            AudioParams::default(),
            EffectStack::default(),
        ));

        if let Some(t) = text {
            self.world.add_component(entity, t);
        }

        self.update_total_frames();
        id
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
    }

    pub fn zoom(&self) -> f32 {
        self.world
            .run(|timeline: UniqueView<TimelineResource>| timeline.zoom_scale)
    }

    pub fn set_layer_visible(&mut self, layer: usize, visible: bool) {
        self.world
            .run(|mut states: UniqueViewMut<LayerStates>| states.set_visible(layer, visible));
    }

    pub fn set_layer_locked(&mut self, layer: usize, locked: bool) {
        self.world
            .run(|mut states: UniqueViewMut<LayerStates>| states.set_locked(layer, locked));
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
    }

    pub fn set_resolution(&mut self, width: u32, height: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.width = width;
                project.height = height;
            });
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
    }

    pub fn set_audio_format(&mut self, sample_rate: u32, channels: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.audio_sample_rate = sample_rate;
                project.audio_channels = channels;
            });
    }

    /// アクティブシーンの解像度・FPSをProjectResourceへ確定反映する唯一の窓口。
    /// switch_scene・update_scene_settings・restore_scenesはすべてここを経由し、
    /// 反映ロジックの重複・乖離を防ぐ。
    fn apply_scene_resolution(&mut self, width: u32, height: u32, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.width = width;
                project.height = height;
                project.fps = fps;
            });
    }

    /// ディスクから復元したシーン一覧・アクティブIDをそのまま反映する（プロジェクト読込直後専用）。
    pub fn restore_scenes(&mut self, active_scene: i32, scenes: Vec<SceneMeta>) {
        self.world.run(|mut res: UniqueViewMut<SceneResource>| {
            let next_scene_id = scenes.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            res.scenes = scenes;
            res.active_scene = active_scene;
            res.next_scene_id = next_scene_id;
        });
        let target = self
            .world
            .run(|scenes: UniqueView<SceneResource>| scenes.find(active_scene).cloned());
        if let Some(scene) = target {
            self.apply_scene_resolution(scene.width, scene.height, scene.fps);
        }
        self.update_total_frames();
    }

    pub fn get_timeline_objects(&self) -> Vec<TimelineData> {
        self.world.run(
            |scenes: UniqueView<SceneResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>,
             kind_ids: View<KindId>,
             layers: View<Layer>,
             scene_ids: View<SceneId>| {
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
                    objs.push(TimelineData {
                        id: id.0 as i32,
                        start_frame: range.start_frame,
                        end_frame: range.end_frame,
                        kind: kind.0 as i32,
                        layer: layer.0,
                    });
                }
                objs
            },
        )
    }

    pub fn move_object(&mut self, object_id: usize, new_start: i32, new_layer: i32) {
        self.world.run(
            |object_ids: View<ObjectId>,
             mut time_ranges: ViewMut<TimeRange>,
             mut layers: ViewMut<Layer>| {
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        if let Ok(mut range) = (&mut time_ranges).get(entity) {
                            let dur = range.end_frame - range.start_frame;
                            range.start_frame = new_start;
                            range.end_frame = new_start + dur;
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
    }

    pub fn resize_object(&mut self, object_id: usize, new_start: i32, new_end: i32) {
        self.world.run(
            |object_ids: View<ObjectId>, mut time_ranges: ViewMut<TimeRange>| {
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        if let Ok(mut range) = (&mut time_ranges).get(entity) {
                            range.start_frame = new_start.max(0);
                            range.end_frame = new_end.max(range.start_frame + 1);
                        }
                        break;
                    }
                }
            },
        );
        self.update_total_frames();
    }

    pub fn find_object_at(&self, frame: i32, layer: i32) -> i32 {
        self.world.run(
            |scenes: UniqueView<SceneResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>,
             layers: View<Layer>,
             scene_ids: View<SceneId>| {
                let active = scenes.active_scene;
                for (_entity, (id, range, l, s)) in (&object_ids, &time_ranges, &layers, &scene_ids)
                    .iter()
                    .with_id()
                {
                    if s.0 == active
                        && l.0 == layer
                        && frame >= range.start_frame
                        && frame < range.end_frame
                    {
                        return id.0 as i32;
                    }
                }
                -1
            },
        )
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

    // --- Transform / GlobalMatrix ---

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

    pub fn get_global_matrix(&self, object_id: usize) -> Option<[f32; 16]> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|matrices: View<GlobalMatrix>| matrices.get(entity).ok().map(|m| m.0))
    }

    // --- EffectStack ---

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
    }

    pub fn reorder_effect(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                if from < stack.0.len() && to < stack.0.len() {
                    let item = stack.0.remove(from);
                    stack.0.insert(to, item);
                }
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
    }

    pub fn get_effects(&self, object_id: usize) -> Vec<EffectInstance> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks.get(entity).map(|s| s.0.clone()).unwrap_or_default()
        })
    }

    // --- TextContent ---

    pub fn get_text(&self, object_id: usize) -> Option<TextContent> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|texts: View<TextContent>| texts.get(entity).ok().cloned())
    }

    pub fn set_text(&mut self, object_id: usize, text: String, x: f32, y: f32, font_size: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut texts: ViewMut<TextContent>| {
            if let Ok(mut slot) = (&mut texts).get(entity) {
                slot.text = text;
                slot.x = x;
                slot.y = y;
                slot.font_size = font_size;
            }
        });
    }

    // --- AudioParams ---

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

    // --- Scene ---

    /// 新規シーンを追加する。幅・高さ・FPSはプロジェクトの現在値を初期値として継承する
    /// （AviQtl::UI::TimelineService::createSceneInternal相当。既定値はグローバル設定でなくプロジェクト値を採用）。
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

    pub fn remove_scene(&mut self, scene_id: i32) {
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
                scenes.active_scene = scenes.scenes.first().map(|s| s.id).unwrap_or(0);
            }
        });
    }

    /// シーン切替。解像度・FPSはシーンの値をプロジェクト設定へ確実に反映する
    /// （プレビュー・レンダーエンジンはProjectResourceを参照するため、
    /// 呼び出し元はこの後にレンダーターゲット再構築を行うこと）。
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
                    prev.layer_states = current_states.clone();
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

    /// シーン設定ウィンドウの確定内容を反映する。
    /// 幅・高さ・FPSはシーン単位のメタデータとして保持し、
    /// アクティブシーンの場合はプロジェクト設定（プレビュー解像度・出力FPS）へも即時反映する。
    pub fn update_scene_settings(&mut self, scene_id: i32, s: SceneSettings) -> bool {
        let updated = self
            .world
            .run(|mut scenes: UniqueViewMut<SceneResource>| -> bool {
                let Some(meta) = scenes.find_mut(scene_id) else {
                    return false;
                };
                meta.name = s.name.clone();
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
}
