// src/ecs/mod.rs
pub mod components;
pub mod resources;
pub mod systems;

use components::{RenderKind, TextContent, TimeRange};
use resources::{ProjectResource, TimelineResource};

pub struct EcsWorld {
    // ── エンティティ並列ストレージ ──
    pub entities: Vec<usize>,
    pub time_ranges: Vec<TimeRange>,
    pub render_kinds: Vec<RenderKind>,
    /// テキストオブジェクトのみ Some。図形オブジェクトは None。
    pub text_contents: Vec<Option<TextContent>>,

    // ── リソース（プロジェクト全体の状態） ──
    pub resources: TimelineResource,
    /// FPS・解像度など動画プロジェクト設定
    pub project: ProjectResource,
}

impl EcsWorld {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            time_ranges: Vec::new(),
            render_kinds: Vec::new(),
            text_contents: Vec::new(),
            resources: TimelineResource::new(),
            project: ProjectResource::new(),
        }
    }

    /// オブジェクトを追加する。テキストオブジェクトの場合は text に Some を渡す。
    pub fn add_object(
        &mut self,
        start: i32,
        duration: i32,
        kind: RenderKind,
        text: Option<TextContent>,
    ) -> usize {
        let id = self.resources.next_id;
        self.resources.next_id += 1;

        self.entities.push(id);
        self.time_ranges.push(TimeRange {
            start_frame: start,
            end_frame: start + duration,
        });
        self.render_kinds.push(kind);
        self.text_contents.push(text);

        self.update_total_frames_pub();
        id
    }

    /// 指定 id のオブジェクトを削除する。
    pub fn delete_object(&mut self, id: usize) {
        if let Some(index) = self.entities.iter().position(|&e_id| e_id == id) {
            self.entities.remove(index);
            self.time_ranges.remove(index);
            self.render_kinds.remove(index);
            self.text_contents.remove(index);
            self.update_total_frames_pub();
        }
    }

    /// total_frames を全オブジェクトの end_frame の最大値（最低 300）に更新する。
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
