use crate::document::{MediaSourceDoc, ObjectDoc, ObjectPayload};
use crate::ecs::EcsWorld;
use crate::ecs::audio_plugins::PluginChain;
use crate::ecs::components::ParamAccess;
use crate::ecs::components::{
    AudioParams, ClipTarget, GroupControl, KeyframeTracks, KindId, Layer, MediaSource, ObjectId,
    PluginParams, SceneId, SceneObject, ShapeParams, TextContent, TimeRange,
};
use crate::ecs::effects::EffectStack;
use crate::ecs::object_query_views::ObjectQueryViews;
use crate::ecs::resolve_stable_id;
use crate::ecs::resources::{SceneResource, TimelineResource};
use crate::ecs::transform::{GlobalMatrix, Transform};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut};
use std::collections::HashMap;

impl EcsWorld {
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

    pub(crate) fn release_media_instance(&self, entity: shipyard::EntityId) {
        self.world.run(
            |media_sources: View<MediaSource>, object_ids: View<ObjectId>| {
                if let (Ok(src), Ok(obj_id)) = (media_sources.get(entity), object_ids.get(entity)) {
                    neoutl_media_runtime::cache::global()
                        .release_instance(&src.path, obj_id.0 as u64);
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
            self.release_media_instance(entity);
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
                self.release_media_instance(entity);
                self.world.delete_entity(entity);
            }
        }
        self.update_total_frames();
        self.touch();
    }

    pub(crate) fn spawn_object_from_doc(&mut self, o: &ObjectDoc) -> shipyard::EntityId {
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
}
