// src/app_state.rs
// 複数プロジェクト同時オープンを管理する。1セッション = 1プロジェクト = 1EcsWorld + 1RenderEngine枠。
use crate::ecs::EcsWorld;
use crate::project::ProjectMeta;
use crate::renderer::RenderEngine;
use std::sync::{Arc, Mutex};

pub struct ProjectSession {
    pub meta: ProjectMeta,
    pub world: Arc<Mutex<EcsWorld>>,
    pub engine: Arc<Mutex<Option<RenderEngine>>>,
}

impl ProjectSession {
    pub fn new(meta: ProjectMeta) -> Self {
        let mut world = EcsWorld::new();
        world.set_project_meta(meta.name.clone(), meta.dir.clone());
        world.set_fps(meta.fps);
        world.set_resolution(meta.width, meta.height);
        world.set_audio_format(meta.audio_sample_rate, meta.audio_channels);
        Self {
            meta,
            world: Arc::new(Mutex::new(world)),
            engine: Arc::new(Mutex::new(None)),
        }
    }
}

pub struct AppState {
    pub sessions: Vec<ProjectSession>,
    pub active: usize,
}

pub type SharedAppState = Arc<Mutex<AppState>>;

impl AppState {
    pub fn new(first: ProjectSession) -> SharedAppState {
        Arc::new(Mutex::new(Self {
            sessions: vec![first],
            active: 0,
        }))
    }
}

/// アクティブセッションのEcsWorldを取得する。
pub fn active_world(state: &SharedAppState) -> Arc<Mutex<EcsWorld>> {
    let s = state.lock().unwrap();
    s.sessions[s.active].world.clone()
}

/// アクティブセッションのRenderEngine枠を取得する。
pub fn active_engine(state: &SharedAppState) -> Arc<Mutex<Option<RenderEngine>>> {
    let s = state.lock().unwrap();
    s.sessions[s.active].engine.clone()
}

/// システム設定は全プロジェクト共通のため、先頭セッションのEcsWorldへ固定する。
pub fn settings_world(state: &SharedAppState) -> Arc<Mutex<EcsWorld>> {
    let s = state.lock().unwrap();
    s.sessions[0].world.clone()
}
