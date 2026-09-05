use crate::document::{DocumentModel, MediaSourceDoc, ObjectDoc, ObjectPayload};
use crate::ecs::EcsWorld;
use crate::ecs::components::ObjectId;
use crate::ecs::object_query_views::ObjectQueryViews;
use crate::ecs::resolve_stable_id;
use crate::ecs::resources::{ProjectResource, SceneResource, TimelineResource};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View};

impl EcsWorld {
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
