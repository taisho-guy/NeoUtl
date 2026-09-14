use crate::project::{self, ProjectMeta};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::NameAsc => SortKey::NameDesc,
            SortKey::NameDesc => SortKey::DateAsc,
            SortKey::DateAsc => SortKey::DateDesc,
            SortKey::DateDesc => SortKey::NameAsc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::NameAsc => "並べ替え：名前 ↑",
            SortKey::NameDesc => "並べ替え：名前 ↓",
            SortKey::DateAsc => "並べ替え：更新日時 ↑",
            SortKey::DateDesc => "並べ替え：更新日時 ↓",
        }
    }

    pub fn apply(self, list: &mut [ProjectMeta]) {
        match self {
            SortKey::NameAsc => list.sort_by(|a, b| a.name.cmp(&b.name)),
            SortKey::NameDesc => list.sort_by(|a, b| b.name.cmp(&a.name)),
            SortKey::DateAsc => list.sort_by_key(|p| p.modified),
            SortKey::DateDesc => list.sort_by_key(|p| std::cmp::Reverse(p.modified)),
        }
    }
}

pub struct LauncherPanel {
    pub name: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub status: String,
    pub search: String,
    pub sort: SortKey,
    pub selection_mode: bool,
    pub selected: HashSet<PathBuf>,
}

impl LauncherPanel {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            fps: 30,
            width: 1920,
            height: 1080,
            sample_rate: 48000,
            channels: 2,
            status: String::new(),
            search: String::new(),
            sort: SortKey::NameAsc,
            selection_mode: false,
            selected: HashSet::new(),
        }
    }

    pub fn create_project(&mut self) -> Result<ProjectMeta, String> {
        match project::create_project(
            &self.name,
            self.fps,
            self.width,
            self.height,
            self.sample_rate,
            self.channels,
        ) {
            Ok(meta) => {
                self.status.clear();
                Ok(meta)
            }
            Err(err) => {
                let msg = err.to_string();
                self.status = msg.clone();
                Err(msg)
            }
        }
    }

    pub fn get_project_list(&self) -> Vec<ProjectMeta> {
        let mut list = project::list_projects();
        if !self.search.is_empty() {
            let q = self.search.to_lowercase();
            list.retain(|p| p.name.to_lowercase().contains(&q));
        }
        self.sort.apply(&mut list);
        list
    }

    pub fn delete_selected(&mut self) {
        for dir in self.selected.drain() {
            let _ = project::delete_project(&dir);
        }
    }

    pub fn copy_selected(&mut self) {
        for dir in self.selected.iter() {
            let _ = project::copy_project(dir);
        }
        self.selected.clear();
    }
}
