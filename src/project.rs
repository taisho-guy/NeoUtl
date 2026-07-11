// src/project.rs
use crate::ecs::resources::SceneMeta;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ProjectMeta {
    pub name: String,
    pub dir: PathBuf,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
}

#[derive(Serialize, Deserialize)]
struct SceneRecord {
    id: i32,
    name: String,
    width: u32,
    height: u32,
    fps: u32,
    grid_mode: i32,
    grid_bpm: f32,
    grid_offset: f32,
    grid_interval: i32,
    grid_subdivision: i32,
    enable_snap: bool,
    magnetic_snap_range: i32,
}

impl From<&SceneMeta> for SceneRecord {
    fn from(s: &SceneMeta) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            width: s.width,
            height: s.height,
            fps: s.fps,
            grid_mode: s.grid_mode,
            grid_bpm: s.grid_bpm,
            grid_offset: s.grid_offset,
            grid_interval: s.grid_interval,
            grid_subdivision: s.grid_subdivision,
            enable_snap: s.enable_snap,
            magnetic_snap_range: s.magnetic_snap_range,
        }
    }
}

impl SceneRecord {
    fn into_meta(self) -> SceneMeta {
        SceneMeta::from_saved(
            self.id,
            self.name,
            self.width,
            self.height,
            self.fps,
            self.grid_mode,
            self.grid_bpm,
            self.grid_offset,
            self.grid_interval,
            self.grid_subdivision,
            self.enable_snap,
            self.magnetic_snap_range,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    name: String,
    fps: u32,
    width: u32,
    height: u32,
    audio_sample_rate: u32,
    audio_channels: u32,
    active_scene: i32,
    scenes: Vec<SceneRecord>,
}

pub fn projects_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("projects")))
        .unwrap_or_else(|| PathBuf::from("projects"))
}

fn meta_path(dir: &Path) -> PathBuf {
    dir.join("project.yaml")
}

fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned
    }
}

pub fn load_project(dir: &Path) -> Option<ProjectMeta> {
    let content = std::fs::read_to_string(meta_path(dir)).ok()?;
    let file: ProjectFile = rust_yaml::from_str(&content).ok()?;
    Some(ProjectMeta {
        name: file.name,
        dir: dir.to_path_buf(),
        fps: file.fps,
        width: file.width,
        height: file.height,
        audio_sample_rate: file.audio_sample_rate,
        audio_channels: file.audio_channels,
    })
}

/// プロジェクトディレクトリからシーン一覧・アクティブIDを復元する。
pub fn load_scenes(dir: &Path) -> Option<(i32, Vec<SceneMeta>)> {
    let content = std::fs::read_to_string(meta_path(dir)).ok()?;
    let file: ProjectFile = rust_yaml::from_str(&content).ok()?;
    Some((
        file.active_scene,
        file.scenes
            .into_iter()
            .map(SceneRecord::into_meta)
            .collect(),
    ))
}

pub fn list_projects() -> Vec<ProjectMeta> {
    let base = projects_dir();
    let Ok(entries) = std::fs::read_dir(&base) else {
        return Vec::new();
    };

    let mut list: Vec<ProjectMeta> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|dir| load_project(&dir))
        .collect();

    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

pub fn create_project(
    name: &str,
    fps: u32,
    width: u32,
    height: u32,
    audio_sample_rate: u32,
    audio_channels: u32,
) -> std::io::Result<ProjectMeta> {
    let base_name = sanitize_dir_name(name);
    let base_dir = projects_dir();
    std::fs::create_dir_all(&base_dir)?;

    let mut dir = base_dir.join(&base_name);
    let mut suffix = 2;
    while dir.exists() {
        dir = base_dir.join(format!("{base_name}_{suffix}"));
        suffix += 1;
    }

    std::fs::create_dir_all(&dir)?;
    let meta = ProjectMeta {
        name: name.trim().to_string(),
        dir,
        fps,
        width,
        height,
        audio_sample_rate,
        audio_channels,
    };
    save_project(&meta, 0, &[SceneMeta::new(0, "Scene 1")])?;
    Ok(meta)
}

/// プロジェクトメタ・全シーン設定（解像度・FPS・グリッド）をディスクへ確定する。
pub fn save_project(
    meta: &ProjectMeta,
    active_scene: i32,
    scenes: &[SceneMeta],
) -> std::io::Result<()> {
    let file = ProjectFile {
        name: meta.name.clone(),
        fps: meta.fps,
        width: meta.width,
        height: meta.height,
        audio_sample_rate: meta.audio_sample_rate,
        audio_channels: meta.audio_channels,
        active_scene,
        scenes: scenes.iter().map(SceneRecord::from).collect(),
    };
    let yaml = rust_yaml::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(meta_path(&meta.dir), yaml)
}
