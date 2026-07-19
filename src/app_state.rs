use crate::config;
use crate::document::DocumentModel;
use crate::ecs::EcsWorld;
use crate::project::{self, ProjectMeta};
use crate::renderer::RenderEngine;
use std::sync::{Arc, Mutex};

/// Undo可能な正本データ（DocumentModel）のスナップショット履歴。
/// ECS(EcsWorld)は焼き込み済み描画状態のためUndo対象に含めない。
pub struct History {
    undo_stack: Vec<DocumentModel>,
    redo_stack: Vec<DocumentModel>,
    limit: usize,
}

impl History {
    fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: config::UNDO_HISTORY_LIMIT,
        }
    }

    /// 編集操作の直前状態を積む。以後のredo系列は破棄する。
    fn push(&mut self, snapshot: DocumentModel) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn undo(&mut self, current: DocumentModel) -> Option<DocumentModel> {
        let prev = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(prev)
    }

    fn redo(&mut self, current: DocumentModel) -> Option<DocumentModel> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(next)
    }
}

pub struct ProjectSession {
    pub meta: ProjectMeta,
    pub world: Arc<Mutex<EcsWorld>>,
    pub engine: Arc<Mutex<Option<RenderEngine>>>,
    pub history: History,
}

impl ProjectSession {
    pub fn new(meta: ProjectMeta) -> Self {
        let mut world = EcsWorld::new();
        world.set_project_meta(meta.name.clone(), meta.dir.clone());
        world.set_fps(meta.fps);
        world.set_resolution(meta.width, meta.height);
        world.set_audio_format(meta.audio_sample_rate, meta.audio_channels);

        if let Some(doc) = project::load_document(&meta.dir) {
            world.load_document(&doc);
        }

        Self {
            meta,
            world: Arc::new(Mutex::new(world)),
            engine: Arc::new(Mutex::new(None)),
            history: History::new(),
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

/// 編集操作の直前に必ず呼ぶ。現在のDocumentModelをUndoスタックへ退避する。
/// UI層の各コールバック冒頭（world変更の直前）に配置する。
pub fn snapshot_before_edit(state: &SharedAppState) {
    let world_holder = active_world(state);
    let snapshot = world_holder.lock().unwrap().to_document();
    let mut s = state.lock().unwrap();
    let active = s.active;
    s.sessions[active].history.push(snapshot);
}

/// アクティブセッションをUndoし、EcsWorldへ再焼き込みする。実行有無を返す。
pub fn undo_active(state: &SharedAppState) -> bool {
    let world_holder = active_world(state);
    let current = world_holder.lock().unwrap().to_document();
    let restored = {
        let mut s = state.lock().unwrap();
        let active = s.active;
        s.sessions[active].history.undo(current)
    };
    let Some(doc) = restored else {
        return false;
    };
    let mut world = world_holder.lock().unwrap();
    world.load_document(&doc);
    let _ = project::save_from_world(&world);
    true
}

/// アクティブセッションをRedoし、EcsWorldへ再焼き込みする。実行有無を返す。
pub fn redo_active(state: &SharedAppState) -> bool {
    let world_holder = active_world(state);
    let current = world_holder.lock().unwrap().to_document();
    let restored = {
        let mut s = state.lock().unwrap();
        let active = s.active;
        s.sessions[active].history.redo(current)
    };
    let Some(doc) = restored else {
        return false;
    };
    let mut world = world_holder.lock().unwrap();
    world.load_document(&doc);
    let _ = project::save_from_world(&world);
    true
}
