// src/project.rs
use crate::config_format;
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

pub fn projects_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("projects")))
        .unwrap_or_else(|| PathBuf::from("projects"))
}

fn meta_path(dir: &Path) -> PathBuf {
    dir.join("project.toml")
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

fn serialize(meta: &ProjectMeta) -> String {
    config_format::format_kv(&[
        ("name", config_format::quote(&meta.name)),
        ("fps", meta.fps.to_string()),
        ("width", meta.width.to_string()),
        ("height", meta.height.to_string()),
        ("audio_sample_rate", meta.audio_sample_rate.to_string()),
        ("audio_channels", meta.audio_channels.to_string()),
    ])
}

pub fn load_project(dir: &Path) -> Option<ProjectMeta> {
    let content = std::fs::read_to_string(meta_path(dir)).ok()?;
    let map = config_format::parse_kv(&content);
    let fallback_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    Some(ProjectMeta {
        name: config_format::get_string(&map, "name", &fallback_name),
        dir: dir.to_path_buf(),
        fps: config_format::get_u32(&map, "fps", 30),
        width: config_format::get_u32(&map, "width", 1920),
        height: config_format::get_u32(&map, "height", 1080),
        audio_sample_rate: config_format::get_u32(&map, "audio_sample_rate", 48000),
        audio_channels: config_format::get_u32(&map, "audio_channels", 2),
    })
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
    std::fs::write(meta_path(&meta.dir), serialize(&meta))?;
    Ok(meta)
}
