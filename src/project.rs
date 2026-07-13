// src/project.rs
use crate::document::{DocumentModel, ObjectDoc};
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

/// ディスク上のプロジェクトファイル形式。DocumentModel（正本データ）をそのまま保持する。
/// `objects`は`#[serde(default)]`により旧形式ファイル（オブジェクト未保存）読込時は空Vecで補完する。
/// `SceneMeta`のランタイム専用フィールド（`total_frames`・`layer_states`）は
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
    next_object_id: usize,
    scenes: Vec<SceneMeta>,
    #[serde(default)]
    objects: Vec<ObjectDoc>,
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

/// プロジェクトディレクトリからDocumentModel（正本データ）全体を復元する。
/// EcsWorld::load_documentへそのまま渡す。
pub fn load_document(dir: &Path) -> Option<DocumentModel> {
    let file = read_file(dir)?;
    Some(DocumentModel {
        project_name: file.name,
        audio_sample_rate: file.audio_sample_rate,
        audio_channels: file.audio_channels,
        active_scene: file.active_scene,
        next_object_id: file.next_object_id,
        scenes: file.scenes,
        objects: file.objects,
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
    let doc = DocumentModel {
        project_name: meta.name.clone(),
        audio_sample_rate,
        audio_channels,
        active_scene: 0,
        next_object_id: 1,
        scenes: vec![{
            let mut s = SceneMeta::new(0, "Scene 1");
            s.width = width;
            s.height = height;
            s.fps = fps;
            s
        }],
        objects: Vec::new(),
    };
    save_document(&meta.dir, &doc)?;
    Ok(meta)
}

/// DocumentModel（正本データ）をディスクへ確定する唯一の窓口。
/// 編集コマンド確定・オートセーブ・Undo/Redo後の再保存等、保存が必要な全箇所からこの関数を呼ぶ。
pub fn save_document(dir: &Path, doc: &DocumentModel) -> std::io::Result<()> {
    let active_scene_meta = doc.scenes.iter().find(|s| s.id == doc.active_scene);
    let file = ProjectFile {
        name: doc.project_name.clone(),
        fps: active_scene_meta.map(|s| s.fps).unwrap_or(30),
        width: active_scene_meta.map(|s| s.width).unwrap_or(1920),
        height: active_scene_meta.map(|s| s.height).unwrap_or(1080),
        audio_sample_rate: doc.audio_sample_rate,
        audio_channels: doc.audio_channels,
        active_scene: doc.active_scene,
        next_object_id: doc.next_object_id,
        scenes: doc.scenes.clone(),
        objects: doc.objects.clone(),
    };
    let yaml = rust_yaml::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(meta_path(dir), yaml)
}

/// EcsWorldの現在状態（DocumentModelへ変換した上で）をディスクへ確定する。
/// プロジェクトディレクトリ未確定（新規未保存等）の場合は何もしない。
pub fn save_from_world(world: &EcsWorld) -> std::io::Result<()> {
    let project = world.get_project();
    let Some(dir) = project.dir else {
        return Ok(());
    };
    save_document(&dir, &world.to_document())
}
