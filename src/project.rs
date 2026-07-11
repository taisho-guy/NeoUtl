// src/project.rs
use crate::ecs::EcsWorld;
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

/// ディスク上のプロジェクトファイル形式。`SceneMeta`をそのまま保持し、
/// ランタイム専用フィールド（`total_frames`・`layer_states`）は
/// `SceneMeta`側の`#[serde(skip)]`で除外される。
#[derive(Serialize, Deserialize)]
struct ProjectFile {
    name: String,
    fps: u32,
    width: u32,
    height: u32,
    audio_sample_rate: u32,
    audio_channels: u32,
    active_scene: i32,
    scenes: Vec<SceneMeta>,
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

fn read_file(dir: &Path) -> Option<ProjectFile> {
    let content = std::fs::read_to_string(meta_path(dir)).ok()?;
    rust_yaml::from_str(&content).ok()
}

pub fn load_project(dir: &Path) -> Option<ProjectMeta> {
    let file = read_file(dir)?;
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
    let file = read_file(dir)?;
    Some((file.active_scene, file.scenes))
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
        scenes: scenes.to_vec(),
    };
    let yaml = rust_yaml::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(meta_path(&meta.dir), yaml)
}

/// EcsWorldの現在状態を唯一の保存窓口としてディスクへ確定する。
/// プロジェクトディレクトリ未確定（新規未保存等）の場合は何もしない。
/// シーン設定変更・オートセーブ等、保存が必要な全箇所からこの関数を呼ぶ。
pub fn save_from_world(world: &EcsWorld) -> std::io::Result<()> {
    let project = world.get_project();
    let Some(dir) = project.dir.clone() else {
        return Ok(());
    };
    let meta = ProjectMeta {
        name: project.name,
        dir,
        fps: project.fps,
        width: project.width,
        height: project.height,
        audio_sample_rate: project.audio_sample_rate,
        audio_channels: project.audio_channels,
    };
    save_project(&meta, world.active_scene(), &world.scenes())
}
