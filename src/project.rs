use crate::document::{DocumentModel, ObjectDoc};
use crate::ecs::EcsWorld;
use crate::ecs::resources::SceneMeta;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn recovery_path(dir: &Path) -> PathBuf {
    dir.join(".recovery").join("autosave.yaml")
}

fn recovery_is_newer(dir: &Path) -> bool {
    let Ok(recovery) = std::fs::metadata(recovery_path(dir)).and_then(|m| m.modified()) else {
        return false;
    };
    let Ok(project) = std::fs::metadata(meta_path(dir)).and_then(|m| m.modified()) else {
        return true;
    };
    recovery > project
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
    let path = if recovery_is_newer(dir) {
        recovery_path(dir)
    } else {
        meta_path(dir)
    };
    let content = std::fs::read_to_string(path).ok()?;
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

pub fn save_document(dir: &Path, doc: &DocumentModel) -> std::io::Result<()> {
    let active_scene_meta = doc.scenes.iter().find(|s| s.id == doc.active_scene);
    let file = ProjectFile {
        name: doc.project_name.clone(),
        fps: active_scene_meta.map_or(30, |s| s.fps),
        width: active_scene_meta.map_or(1920, |s| s.width),
        height: active_scene_meta.map_or(1080, |s| s.height),
        audio_sample_rate: doc.audio_sample_rate,
        audio_channels: doc.audio_channels,
        active_scene: doc.active_scene,
        next_object_id: doc.next_object_id,
        scenes: doc.scenes.clone(),
        objects: doc.objects.clone(),
    };
    let yaml = rust_yaml::to_string(&file).map_err(std::io::Error::other)?;
    write_atomic(&meta_path(dir), &yaml)?;
    clear_recovery(dir);
    Ok(())
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let temp = path.with_extension("yaml.tmp");
    std::fs::write(&temp, content)?;
    std::fs::rename(temp, path)
}

pub fn save_autosave_from_world(world: &EcsWorld) -> std::io::Result<()> {
    let project = world.get_project();
    let Some(dir) = project.dir else {
        return Ok(());
    };
    let recovery = recovery_path(&dir);
    if let Some(parent) = recovery.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = world.to_document();
    let active_scene_meta = doc.scenes.iter().find(|s| s.id == doc.active_scene);
    let file = ProjectFile {
        name: doc.project_name.clone(),
        fps: active_scene_meta.map_or(30, |s| s.fps),
        width: active_scene_meta.map_or(1920, |s| s.width),
        height: active_scene_meta.map_or(1080, |s| s.height),
        audio_sample_rate: doc.audio_sample_rate,
        audio_channels: doc.audio_channels,
        active_scene: doc.active_scene,
        next_object_id: doc.next_object_id,
        scenes: doc.scenes,
        objects: doc.objects,
    };
    let yaml = rust_yaml::to_string(&file).map_err(std::io::Error::other)?;
    write_atomic(&recovery, &yaml)
}

pub fn clear_recovery(dir: &Path) {
    let _ = std::fs::remove_file(recovery_path(dir));
    let _ = std::fs::remove_dir(dir.join(".recovery"));
}

pub fn runtime_marker_path() -> PathBuf {
    settings_runtime_dir().join("running.lock")
}

fn settings_runtime_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("settings")))
        .unwrap_or_else(|| PathBuf::from("settings"))
}

pub fn begin_runtime_session() -> std::io::Result<bool> {
    let path = runtime_marker_path();
    let crashed = path.exists();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!(
        "{}\n{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    write_atomic(&path, &content)?;
    Ok(crashed)
}

pub fn finish_runtime_session() {
    let _ = std::fs::remove_file(runtime_marker_path());
}

pub fn save_from_world(world: &EcsWorld) -> std::io::Result<()> {
    let project = world.get_project();
    let Some(dir) = project.dir else {
        return Ok(());
    };
    save_document(&dir, &world.to_document())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{MediaSourceDoc, ObjectPayload};
    use crate::ecs::components::{AudioParams, ShapeParams, TextContent};
    use crate::ecs::transform::Transform;
    use crate::media::MediaKind;
    use std::collections::HashMap;

    fn sample_object(id: usize, scene_id: i32) -> ObjectDoc {
        ObjectDoc {
            id,
            scene_id,
            kind_id: 1,
            layer: 0,
            start_frame: 0,
            end_frame: 30,
            transform: Transform::default(),
            audio: AudioParams::default(),
            effects: Vec::new(),
            payload: ObjectPayload {
                text: Some(TextContent::default()),
                shape: None,
                plugin_params: None,
                media: None,
            },
            keyframes: HashMap::new(),
        }
    }

    fn sample_shape_object(id: usize, scene_id: i32) -> ObjectDoc {
        ObjectDoc {
            id,
            scene_id,
            kind_id: 2,
            layer: 1,
            start_frame: 30,
            end_frame: 90,
            transform: Transform::default(),
            audio: AudioParams::default(),
            effects: Vec::new(),
            payload: ObjectPayload {
                text: None,
                shape: Some(ShapeParams::default()),
                plugin_params: None,
                media: Some(MediaSourceDoc {
                    path: PathBuf::from("dummy.png"),
                    kind: MediaKind::Image,
                    trim_in_frame: 0,
                }),
            },
            keyframes: HashMap::new(),
        }
    }

    #[test]
    fn roundtrip_create_load() {
        let name = format!(
            "neoutl_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let meta = create_project(&name, 30, 1920, 1080, 48000, 2).unwrap();
        let loaded = load_document(&meta.dir).unwrap();
        assert_eq!(loaded.project_name, name);
        assert_eq!(loaded.audio_sample_rate, 48000);
        assert_eq!(loaded.audio_channels, 2);
        assert_eq!(loaded.active_scene, 0);
        assert_eq!(loaded.next_object_id, 1);
        assert_eq!(loaded.scenes.len(), 1);
        assert_eq!(loaded.scenes[0].width, 1920);
        assert_eq!(loaded.scenes[0].height, 1080);
        assert_eq!(loaded.scenes[0].fps, 30);
        assert!(loaded.objects.is_empty());
        std::fs::remove_dir_all(&meta.dir).ok();
    }

    #[test]
    fn roundtrip_save_load_with_objects() {
        let dir = tempfile::tempdir().unwrap();
        let doc = DocumentModel {
            project_name: "t2".to_string(),
            audio_sample_rate: 44100,
            audio_channels: 1,
            active_scene: 0,
            next_object_id: 3,
            scenes: vec![SceneMeta::new(0, "Scene 1")],
            objects: vec![sample_object(1, 0), sample_shape_object(2, 0)],
        };
        save_document(dir.path(), &doc).unwrap();
        let loaded = load_document(dir.path()).unwrap();
        assert_eq!(loaded.objects.len(), 2);
        assert_eq!(loaded.objects[0].id, 1);
        assert_eq!(loaded.objects[0].kind_id, 1);
        assert!(loaded.objects[0].payload.text.is_some());
        assert_eq!(loaded.objects[1].id, 2);
        assert_eq!(loaded.objects[1].kind_id, 2);
        assert!(loaded.objects[1].payload.shape.is_some());
        assert!(loaded.objects[1].payload.media.is_some());
        assert_eq!(
            loaded.objects[1]
                .payload
                .media
                .as_ref()
                .unwrap()
                .trim_in_frame,
            0
        );
    }

    #[test]
    fn legacy_format_without_objects_field() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_yaml = "name: legacy\nfps: 24\nwidth: 640\nheight: 480\naudio_sample_rate: 48000\naudio_channels: 2\nactive_scene: 0\nnext_object_id: 1\nscenes:\n  - id: 0\n    name: Scene 1\n    width: 640\n    height: 480\n    fps: 24\n    grid_mode: 0\n    grid_bpm: 120.0\n    grid_offset: 0.0\n    grid_interval: 30\n    grid_subdivision: 1\n    enable_snap: true\n    magnetic_snap_range: 5\n";
        std::fs::write(meta_path(dir.path()), legacy_yaml).unwrap();
        let loaded = load_document(dir.path()).unwrap();
        assert!(loaded.objects.is_empty());
        assert_eq!(loaded.project_name, "legacy");
    }

    #[test]
    fn sanitize_dir_name_keeps_unicode_alnum() {
        let name = "コリジョン";
        let cleaned = sanitize_dir_name(name);
        assert_eq!(cleaned, name);
    }

    #[test]
    fn sanitize_dir_name_replaces_path_separators() {
        let name = "a/b\\c";
        let cleaned = sanitize_dir_name(name);
        assert_eq!(cleaned, "a_b_c");
    }

    #[test]
    fn sanitize_dir_name_empty_falls_back() {
        let cleaned = sanitize_dir_name("   ");
        assert_eq!(cleaned, "project");
    }

    #[test]
    fn create_project_dir_collision_appends_suffix() {
        let name = format!(
            "neoutl_test_collision_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let meta1 = create_project(&name, 30, 1920, 1080, 48000, 2).unwrap();
        let meta2 = create_project(&name, 30, 1920, 1080, 48000, 2).unwrap();
        assert_ne!(meta1.dir, meta2.dir);
        assert!(
            meta2
                .dir
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("_2")
        );
        std::fs::remove_dir_all(&meta1.dir).ok();
        std::fs::remove_dir_all(&meta2.dir).ok();
    }
}
