pub mod components;
pub mod resources;
pub mod systems;

use components::TimeRange;
use resources::TimelineResource;

pub struct EcsWorld {
    pub entities: Vec<usize>,        // Entity IDの配列
    pub time_ranges: Vec<TimeRange>, // 各Entityの時間範囲コンポーネント
    pub resources: TimelineResource, // グローバルリソース
}

impl EcsWorld {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            time_ranges: Vec::new(),
            resources: TimelineResource::new(),
        }
    }

    /// 新しいオブジェクトを生成・追加
    pub fn add_object(&mut self, start: i32, duration: i32) -> usize {
        let id = self.resources.next_id;
        self.resources.next_id += 1;

        self.entities.push(id);
        self.time_ranges.push(TimeRange {
            start_frame: start,
            end_frame: start + duration,
        });

        self.update_total_frames();
        id
    }

    /// 指定されたIDのオブジェクトをコンポーネント配列から削除
    pub fn delete_object(&mut self, id: usize) {
        if let Some(index) = self.entities.iter().position(|&e_id| e_id == id) {
            self.entities.remove(index);
            self.time_ranges.remove(index); // 隙間を詰めて連続性を維持
            self.update_total_frames();
        }
    }

    fn update_total_frames(&mut self) {
        let max_end = self
            .time_ranges
            .iter()
            .map(|t| t.end_frame)
            .max()
            .unwrap_or(0);
        self.resources.total_frames = max_end.max(300);
    }
}
