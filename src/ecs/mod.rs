// src/ecs/mod.rs
pub mod components;
pub mod resources;
pub mod systems;

use components::{KindId, ObjectId, TextContent, TimeRange};
use resources::{ProjectResource, TimelineResource};
use shipyard::{Get, IntoIter, UniqueView, UniqueViewMut, View, World};

pub struct EcsWorld {
    pub world: World,
}

impl EcsWorld {
    pub fn new() -> Self {
        let world = World::new();
        world.add_unique(TimelineResource::new());
        world.add_unique(ProjectResource::new());
        Self { world }
    }

    pub fn add_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        text: Option<TextContent>,
    ) -> usize {
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

    pub fn get_timeline_objects(&self) -> Vec<crate::TimelineObject> {
        self.world.run(
            |object_ids: View<ObjectId>, time_ranges: View<TimeRange>, kind_ids: View<KindId>| {
                let mut objs = Vec::new();
                for (_entity, (id, range, kind)) in
                    (&object_ids, &time_ranges, &kind_ids).iter().with_id()
                {
                    let label = crate::objects::registry()
                        .get(kind.0 as usize)
                        .map(|p| p.name.as_str())
                        .unwrap_or("Unknown");
                    objs.push(crate::TimelineObject {
                        id: id.0 as i32,
                        start_frame: range.start_frame,
                        end_frame: range.end_frame,
                        kind: kind.0 as i32,
                        label: label.into(),
                    });
                }
                objs
            },
        )
    }

    pub fn move_object(&mut self, object_id: usize, new_start: i32) {
        self.world.run(
            |object_ids: View<ObjectId>, mut time_ranges: shipyard::ViewMut<TimeRange>| {
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        if let Ok(ref mut range) = (&mut time_ranges).get(entity) {
                            let dur = range.end_frame - range.start_frame;
                            range.start_frame = new_start;
                            range.end_frame = new_start + dur;
                        }
                        break;
                    }
                }
            },
        );
        self.update_total_frames();
    }

    pub fn find_object_at(&self, ratio: f32) -> i32 {
        self.world.run(
            |timeline: UniqueView<TimelineResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>| {
                let total = timeline.total_frames as f32;
                let frame = (ratio * total) as i32;
                for (_entity, (id, range)) in (&object_ids, &time_ranges).iter().with_id() {
                    if frame >= range.start_frame && frame < range.end_frame {
                        return id.0 as i32;
                    }
                }
                -1
            },
        )
    }

    pub fn get_object_start_ratio(&self, object_id: usize) -> f32 {
        self.world.run(
            |timeline: UniqueView<TimelineResource>,
             object_ids: View<ObjectId>,
             time_ranges: View<TimeRange>| {
                let total = timeline.total_frames as f32;
                if total <= 0.0 {
                    return 0.0;
                }
                for (entity, id) in object_ids.iter().with_id() {
                    if id.0 == object_id {
                        if let Ok(range) = time_ranges.get(entity) {
                            return range.start_frame as f32 / total;
                        }
                    }
                }
                0.0
            },
        )
    }
}
