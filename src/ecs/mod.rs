// src/ecs/mod.rs
pub mod components;
pub mod resources;
pub mod systems;

use components::{RenderKind, TimeRange};
use resources::TimelineResource;

pub struct EcsWorld {
    pub entities: Vec<usize>,
    pub time_ranges: Vec<TimeRange>,
    pub render_kinds: Vec<RenderKind>,
    pub resources: TimelineResource,
}

impl EcsWorld {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            time_ranges: Vec::new(),
            render_kinds: Vec::new(),
            resources: TimelineResource::new(),
        }
    }

    pub fn add_object(&mut self, start: i32, duration: i32, kind: RenderKind) -> usize {
        let id = self.resources.next_id;
        self.resources.next_id += 1;

        self.entities.push(id);
        self.time_ranges.push(TimeRange {
            start_frame: start,
            end_frame: start + duration,
        });
        self.render_kinds.push(kind);

        self.update_total_frames_pub();
        id
    }

    pub fn delete_object(&mut self, id: usize) {
        if let Some(index) = self.entities.iter().position(|&e_id| e_id == id) {
            self.entities.remove(index);
            self.time_ranges.remove(index);
            self.render_kinds.remove(index);
            self.update_total_frames_pub();
        }
    }

    /// total_frames を全オブジェクトの end_frame の最大値（最低300）に更新する。
    /// ui/mod.rs からも呼び出せるよう pub に変更。
    pub fn update_total_frames_pub(&mut self) {
        let max_end = self
            .time_ranges
            .iter()
            .map(|t| t.end_frame)
            .max()
            .unwrap_or(0);
        self.resources.total_frames = max_end.max(300);
    }
}
