pub mod audio_plugins;
pub mod components;
pub mod effects;
pub mod object_schema;
pub mod resources;
pub mod systems;
pub mod transform;
pub mod types;

use crate::document::{DocumentModel, MediaSourceDoc, ObjectDoc, ObjectPayload};
use crate::ecs::types::EffectInstance;
use audio_plugins::PluginChain;
use components::{
    AudioParams, KeyframeTracks, KindId, Layer, MediaSource, ObjectId, ParamAccess, PluginParams,
    SceneId, ShapeParams, TextContent, TimeRange,
};
use effects::EffectStack;
use resources::{
    LayerStates, ProjectResource, SceneMeta, SceneResource, SystemSettingsResource,
    TimelineResource,
};
use std::collections::HashMap;

use shipyard::{
    Borrow, BorrowInfo, Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut, World,
};
use transform::{Camera, GlobalMatrix, Transform, compute_global_matrix};

/// to_document()の元Viewを束ねる集約ビュー。
/// shipyard 0.11のSystem<(), B>実装はクロージャ引数個数に上限があり、
/// 11個の個別Viewパラメータはこの上限を超過してコンパイルエラーとなる。
/// 個別Viewを1個の派生Borrow構造体へ集約し、クロージャの引数を1個に圧縮する。
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
}

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
        world.add_unique(Camera::default());
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

    /// 図形オブジェクトを追加する。ShapeParamsコンポーネントを併せて付与する。
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
        id
    }

    /// 動画・画像・音声オブジェクトを追加する。MediaSourceコンポーネントを併せて付与する。
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

    /// 複数オブジェクトの一括削除。個々にdelete_objectを適用する
    /// （1回のみのupdate-total-frames呼び出しで正規化を完結させる）。
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
    }

    pub fn set_audio_format(&mut self, sample_rate: u32, channels: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.audio_sample_rate = sample_rate;
                project.audio_channels = channels;
            });
    }

    /// アクティブシーンの解像度・FPSをProjectResourceへ確定反映する唯一の窓口。
    /// Cameraはproject_width/heightに依存するため、解像度確定のたびにここで
    /// Camera::for_resolution()により必ず再導出する。個別呼び出し側で
    /// Cameraを直接いじる必要はない。
    fn apply_scene_resolution(&mut self, width: u32, height: u32, fps: u32) {
        self.world
            .run(|mut project: UniqueViewMut<ProjectResource>| {
                project.width = width;
                project.height = height;
                project.fps = fps;
            });
        self.set_camera(Camera::for_resolution(width as f32, height as f32));
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

    /// アクティブシーンのグリッド設定に基づき吸着させたフレーム番号を返す。
    /// SceneMeta::enable_snap/magnetic_snap_range/grid_intervalを実際に消費する唯一の経路。
    fn snap_to_active_scene(&self, frame: i32) -> i32 {
        self.world.run(|scenes: UniqueView<SceneResource>| {
            scenes
                .find(scenes.active_scene)
                .map_or(frame, |s| s.snap_frame(frame))
        })
    }

    /// グリッドスナップに続く第2段階の吸着。グリッドで吸着済みの場合はそれを優先し
    /// （両者が競合した場合の挙動を一意に決定するため）、グリッド未吸着の場合のみ
    /// 同一レイヤー上の他クリップ端（start-frame/end-frame）と再生ヘッド位置を
    /// 候補として磁力スナップを試みる。excludeIdは対象クリップ自身を候補から除く。
    /// enable-snap無効時、またはmagnetic-snap-range<=0の場合は吸着しない。
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

    /// クリップの単純平行移動。TimeRangeの変更に合わせて、ネイティブパラメータ
    /// （KeyframeTracks）・エフェクトパラメータ（EffectStack）双方の中間点も
    /// deltaだけシフトする。resize_objectがクランプ・再構築を行うのと対称に、
    /// move_objectは中間点の絶対フレーム位置をクリップ本体と同じ量だけ動かす
    /// （移動は中間点へ影響しない、という非対称設計を解消する）。
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
    }

    /// クリップ伸縮。中間点の境界クランプ則（neoutl_interp::clamp_and_reseed、
    /// 詳細はそのドキュメントコメント参照）をネイティブパラメータ（KeyframeTracks）・
    /// エフェクトパラメータ（EffectStack）双方へ適用する。旧範囲(old_start/old_end)を
    /// TimeRange上書き前に確保してからクランプへ渡すため、内部点は「絶対フレーム
    /// 不変」ではなく「クリップ内相対位置不変」でスケールされる。
    /// 1フレーム未満へ縮む要求はrange.end_frameの下限クランプで最小幅1フレームへ
    /// 丸められ、破綻（0/負幅）を構造的に排除する。
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

    /// リップル移動。対象クリップをmove_objectと同一則で移動し、同一レイヤー上で
    /// 旧start-frame以降にある全クリップ（対象自身を除く）へ移動量deltaをそのまま
    /// 伝播させる（AviUtl「リップル編集」相当）。レイヤーは対象クリップの現在値を
    /// 保持したまま移動する（リップル移動はレイヤー変更を伴わない）。
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

    /// リップル伸縮。対象クリップのend-frameをresize_objectと同一則で変更し、
    /// 同一レイヤー上で旧end-frame以降にある全クリップへ変化量deltaを平行移動
    /// として伝播させる（後続クリップ自体は伸縮せず、位置のみ追従する）。
    /// start-frame側（左端リサイズ）のリップルは対象外とする
    /// （左端はクリップ自身の trim-in のみに影響し、後続位置は変化しないため）。
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

    /// ObjectDocから1エンティティを生成する（load_document/paste_objects共通処理）。
    /// idはo.idをそのまま使用するため、呼び出し側で一意性を保証すること。
    fn spawn_object_from_doc(&mut self, o: &ObjectDoc) -> shipyard::EntityId {
        let entity = self.world.add_entity((
            ObjectId(o.id),
            TimeRange {
                start_frame: o.start_frame,
                end_frame: o.end_frame,
            },
            KindId(o.kind_id),
            Layer(o.layer),
            SceneId(o.scene_id),
            o.transform,
            GlobalMatrix::default(),
            o.audio,
            EffectStack(o.effects.clone()),
        ));
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
            self.world.add_component(entity, PluginChain(chain.clone()));
        }
        if let Some(m) = &o.payload.media {
            self.world.add_component(entity, MediaSource::from(m));
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

    /// idsで指定した全オブジェクトのObjectDocスナップショットを返す（クリップボード用）。
    /// 複数選択（AviQtl::TimelineView::shouldApplyToSelection相当）を前提に、
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
                    kind_id: kind.0,
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
                    },
                });
            }
            docs
        })
    }

    /// クリップボードのdocsをアクティブシーンへ貼り付ける。docs内の最小start-frame・
    /// 最小layerを基準（アンカー）とし、target-frame/target-layerを新アンカーとして
    /// 各オブジェクトの相対位置（複数選択の位置関係）を保ったまま配置する
    /// （AviQtl::TimelineView::pasteClip相当。単一貼り付けもdocs長1の特殊形として扱う）。
    /// 新規idはEcsWorld::next_idから採番し、貼り付け先はアクティブシーンに固定する。
    /// 戻り値は新規生成した全idsで、呼び出し側の選択状態更新に使う。
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

    /// 複数選択オブジェクトの複製。copy_objects→paste_objectsの合成で、
    /// AviQtl::TimelineView::handleCommand("clip.duplicate")と同じくカーソル位置・
    /// 選択レイヤーを新アンカーとして貼り付ける。
    pub fn duplicate_objects(
        &mut self,
        ids: &[usize],
        target_frame: i32,
        target_layer: i32,
    ) -> Vec<usize> {
        let docs = self.copy_objects(ids);
        self.paste_objects(&docs, target_frame, target_layer)
    }

    /// 複数選択オブジェクトの切り取り。コピー内容を返しつつ元オブジェクトを削除する
    /// （呼び出し側でRust側の戻り値をアプリ全体のクリップボード状態へ格納する）。
    pub fn cut_objects(&mut self, ids: &[usize]) -> Vec<ObjectDoc> {
        let docs = self.copy_objects(ids);
        self.delete_objects(ids);
        docs
    }

    /// object_idを絶対フレームsplit_frameで2分割する。前半（元エンティティ）は
    /// [start_frame, split_frame)、後半（新規エンティティ）は[split_frame, end_frame)
    /// を保持する。中間点はAviQtl EffectModel::splitTracks相当のロジックで追従する:
    /// - フレーム番号は絶対値のまま変更しない（apply/evaluateが絶対フレーム基準のため）
    /// - 分割点をまたぐ区間は、分割点での評価値を後半側の基準値（フィールド初期値）へ
    ///   複製し、値が瞬断しないようにする
    /// - 分割点より前の中間点は前半へ、後の中間点は後半へ、それぞれ絶対フレームのまま残す
    /// PluginParams（プラグイン固有パラメータ）はParamAccess非対応のため中間点追従の
    /// 対象外（そのまま複製のみ）。split_frameが区間内部でない場合はNoneを返す。
    /// 戻り値は新規生成したオブジェクトのid。
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

    pub fn get_camera(&self) -> Camera {
        self.world.run(|camera: UniqueView<Camera>| *camera)
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
    }

    /// VST3/CLAPプラグインをPluginChain末尾へ追加する。エンティティ未付与の場合は
    /// PluginChainを新規付与する（EffectStackはadd_object時に必ず付与されるが、
    /// PluginChainはaudioオブジェクトのみが対象のため遅延付与とする）。
    pub fn add_plugin(
        &mut self,
        object_id: usize,
        format: neoutl_audio_plugin_host::PluginFormat,
        path: std::path::PathBuf,
        plugin_id: String,
    ) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        let has_chain = self
            .world
            .run(|chains: View<PluginChain>| chains.get(entity).is_ok());
        if !has_chain {
            self.world.add_component(entity, PluginChain::default());
        }
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.push(format, path, plugin_id);
            }
        });
    }

    pub fn remove_plugin(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.remove(index);
            }
        });
    }

    pub fn reorder_plugin(&mut self, object_id: usize, from: usize, to: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.reorder(from, to);
            }
        });
    }

    pub fn set_plugin_bypass(&mut self, object_id: usize, index: usize, bypass: bool) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.set_bypass(index, bypass);
            }
        });
    }

    pub fn set_plugin_param(&mut self, object_id: usize, index: usize, param_id: u32, value: f64) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut chains: ViewMut<PluginChain>| {
            if let Ok(mut chain) = (&mut chains).get(entity) {
                chain.set_param(index, param_id, value);
            }
        });
    }

    pub fn get_plugin_chain(
        &self,
        object_id: usize,
    ) -> Option<Vec<audio_plugins::PluginInstanceRef>> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|chains: View<PluginChain>| chains.get(entity).ok().map(|c| c.0.clone()))
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
    }

    pub fn get_effects(&self, object_id: usize) -> Vec<EffectInstance> {
        let Some(entity) = self.find_entity(object_id) else {
            return Vec::new();
        };
        self.world.run(|stacks: View<EffectStack>| {
            stacks.get(entity).map(|s| s.0.clone()).unwrap_or_default()
        })
    }

    pub fn get_effect_instance(&self, object_id: usize, index: usize) -> Option<EffectInstance> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|stacks: View<EffectStack>| stacks.get(entity).ok()?.0.get(index).cloned())
    }

    pub fn insert_effect(&mut self, object_id: usize, index: usize, instance: EffectInstance) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.insert(index, instance);
            }
        });
    }

    pub fn duplicate_effect(&mut self, object_id: usize, index: usize) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            if let Ok(mut stack) = (&mut stacks).get(entity) {
                stack.duplicate(index);
            }
        });
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
    }

    pub fn get_shape(&self, object_id: usize) -> Option<ShapeParams> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|shapes: View<ShapeParams>| shapes.get(entity).ok().copied())
    }

    pub fn set_shape(&mut self, object_id: usize, shape: ShapeParams) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut shapes: ViewMut<ShapeParams>| {
            if let Ok(mut slot) = (&mut shapes).get(entity) {
                *slot = shape;
            }
        });
    }

    pub fn get_media(&self, object_id: usize) -> Option<MediaSource> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|media: View<MediaSource>| media.get(entity).ok().cloned())
    }

    pub fn set_media_trim(&mut self, object_id: usize, trim_in_frame: i64) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        self.world.run(|mut media: ViewMut<MediaSource>| {
            if let Ok(mut slot) = (&mut media).get(entity) {
                slot.trim_in_frame = trim_in_frame;
            }
        });
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

    pub fn get_kind_id(&self, object_id: usize) -> Option<u32> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|kinds: View<KindId>| kinds.get(entity).ok().map(|k| k.0))
    }

    pub fn get_plugin_params(&self, object_id: usize) -> Option<HashMap<String, f32>> {
        let entity = self.find_entity(object_id)?;
        self.world
            .run(|params: View<PluginParams>| params.get(entity).ok().map(|p| p.0.clone()))
    }

    /// ネイティブパラメータ（Transform/TextContent/ShapeParams/AudioParams）のkeyへ
    /// 中間点を1件設定する。KeyframeTracks未付与のエンティティには新規付与する
    /// （set_plugin_paramと同一方針: 都度読み出し→書き換え→add_component）。
    /// 評価（描画時の実効値算出）はecs::systems側でのみ行い、ここでは行わない。
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
    }

    /// ネイティブパラメータの中間点をドラッグ移動する。移動先に既存点がある場合は失敗する。
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

    /// オブジェクトの絶対フレーム範囲 (start_frame, end_frame) を返す。
    /// 中間点区間UIの両端境界として使う。エンティティ不在時は(0,1)。
    pub fn get_time_range(&self, object_id: usize) -> (i32, i32) {
        let Some(entity) = self.find_entity(object_id) else {
            return (0, 1);
        };
        self.world
            .run(|t: View<TimeRange>| t.get(entity).ok().map(|r| (r.start_frame, r.end_frame)))
            .unwrap_or((0, 1))
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

    /// エフェクトパラメータへの中間点設定。EffectStack::set_keyframeへ委譲する。
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
    }

    /// エフェクトパラメータの中間点をドラッグ移動する。移動先に既存点がある場合は失敗する。
    pub fn move_effect_keyframe(
        &mut self,
        object_id: usize,
        index: usize,
        key: &str,
        old_frame: i32,
        new_frame: i32,
    ) -> bool {
        let Some(entity) = self.find_entity(object_id) else {
            return false;
        };
        self.world.run(|mut stacks: ViewMut<EffectStack>| {
            (&mut stacks)
                .get(entity)
                .ok()
                .map(|mut s| s.move_keyframe(index, key, old_frame, new_frame))
                .unwrap_or(false)
        })
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

    pub fn set_plugin_param_value(&mut self, object_id: usize, key: &str, value: f32) {
        let Some(entity) = self.find_entity(object_id) else {
            return;
        };
        let mut params = self
            .world
            .run(|p: View<PluginParams>| p.get(entity).ok().map(|s| s.0.clone()))
            .unwrap_or_default();
        params.insert(key.to_string(), value);
        self.world.add_component(entity, PluginParams(params));
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
                scenes.active_scene = scenes.scenes.first().map_or(0, |s| s.id);
            }
        });
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
                    kind_id: kind.0,
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

    /// 既存エンティティは全削除の上、doc.objectsから再生成する
    /// （差分検出をせず毎回全再構築。オブジェクト数が数千規模になるまでは
    /// 個別差分焼き込みより実装単純性を優先する）。
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
    }
}
