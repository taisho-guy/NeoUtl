// src/ecs/mod.rs
pub mod components;
pub mod resources;
pub mod systems;

use components::{KindId, Layer, ObjectId, TextContent, TimeRange};
use resources::{LayerStates, ProjectResource, TimelineResource};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View, ViewMut, World};

#[derive(Clone, Debug)]
pub struct TimelineData {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub layer: i32,
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
        Self { world }
    }

    pub fn add_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        layer: i32,
        text: Option<TextContent>,
    ) -> Option<usize> {
        let layer = layer.max(0);
        let locked = self
            .world
            .run(|states: UniqueView<LayerStates>| states.locked(layer as usize));
        if locked {
            return None;
        }

        let id = self
            .world
            .run(|mut timeline: UniqueViewMut<TimelineResource>| {
                let id = timeline.next_id;
                timeline.next_id += 1;
                id
            });

        let entity = self.world.add_entity((
            ObjectId(id),
            TimeRange {
                start_frame: start,
                end_frame: start + duration,
            },
            KindId(kind_id),
            Layer(layer),
        ));

        if let Some(t) = text {
            self.world.add_component(entity, t);
        }

        self.update_total_frames();
        Some(id)
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

    pub fn get_timeline_objects(&self) -> Vec<TimelineData> {
        self.world.run(
            |object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>,
             kind_ids: View<KindId>,
             layers: View<Layer>| {
                let mut objs = Vec::new();
                for (_entity, (id, range, kind, layer)) in
                    (&object_ids, &time_ranges, &kind_ids, &layers)
                        .iter()
                        .with_id()
                {
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
        let new_layer = new_layer.max(0);
        let locked = self
            .world
            .run(|states: UniqueView<LayerStates>| states.locked(new_layer as usize));
        if locked {
            return;
        }

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
                            layer.0 = new_layer;
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
            |object_ids: View<ObjectId>, time_ranges: View<TimeRange>, layers: View<Layer>| {
                for (_entity, (id, range, l)) in
                    (&object_ids, &time_ranges, &layers).iter().with_id()
                {
                    if l.0 == layer && frame >= range.start_frame && frame < range.end_frame {
                        return id.0 as i32;
                    }
                }
                -1
            },
        )
    }
}
