use crate::ecs::EcsWorld;
use crate::ecs::SceneSettings;
use crate::ecs::components::{ObjectId, SceneId, SceneObject};
use crate::ecs::resources::{LayerStates, SceneMeta, SceneResource, TimelineResource};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View};
use std::collections::HashMap;

impl EcsWorld {
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
            self.release_media_instance(entity);
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
}
