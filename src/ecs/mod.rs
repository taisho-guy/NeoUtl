// src/ecs/mod.rs
pub mod components;
pub mod resources;
pub mod systems;

use components::{TextContent, TimeRange};
use resources::{ProjectResource, TimelineResource};

pub struct EcsWorld {
    pub entities: Vec<usize>,
    pub time_ranges: Vec<TimeRange>,
    pub kind_ids: Vec<u32>,
    pub text_contents: Vec<Option<TextContent>>,
    pub resources: TimelineResource,
    pub project: ProjectResource,
}

impl EcsWorld {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            time_ranges: Vec::new(),
            kind_ids: Vec::new(),
            text_contents: Vec::new(),
            resources: TimelineResource::new(),
            project: ProjectResource::new(),
        }
    }

    pub fn add_object(
        &mut self,
        start: i32,
        duration: i32,
        kind_id: u32,
        text: Option<TextContent>,
    ) -> usize {
        let id = self.resources.next_id;
        self.resources.next_id += 1;

        self.entities.push(id);
        self.time_ranges.push(TimeRange {
            start_frame: start,
            end_frame: start + duration,
        });
        self.kind_ids.push(kind_id);
        self.text_contents.push(text);

        self.update_total_frames();
        id
    }

    pub fn delete_object(&mut self, id: usize) {
        if let Some(i) = self.entities.iter().position(|&e| e == id) {
            self.entities.remove(i);
            self.time_ranges.remove(i);
            self.kind_ids.remove(i);
            self.text_contents.remove(i);
            self.update_total_frames();
        }
    }

    pub fn update_total_frames(&mut self) {
        let max_end = self
            .time_ranges
            .iter()
            .map(|t| t.end_frame)
            .max()
            .unwrap_or(0);
        self.resources.total_frames = max_end.max(300);
    }
}
