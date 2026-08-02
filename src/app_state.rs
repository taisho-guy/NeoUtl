use crate::audio::AudioMixer;
use crate::config;
use crate::document::DocumentModel;
use crate::document::ObjectDoc;
use crate::ecs::EcsWorld;
use crate::project::{self, ProjectMeta};
use crate::renderer::RenderEngine;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
    pub audio_mixer: Arc<Mutex<AudioMixer>>,
    pub history: History,
    pub dirty: bool,
    pub last_autosave: Instant,
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

        let audio_mixer = AudioMixer::new(meta.audio_sample_rate).unwrap_or_else(|err| {
            eprintln!("[NeoUtl] audio_mixer初期化失敗: {err}");
            AudioMixer::silent()
        });

        Self {
            meta,
            world: Arc::new(Mutex::new(world)),
            engine: Arc::new(Mutex::new(None)),
            audio_mixer: Arc::new(Mutex::new(audio_mixer)),
            history: History::new(),
            dirty: false,
            last_autosave: Instant::now(),
        }
    }
}

pub struct AppState {
    pub sessions: Vec<ProjectSession>,
    pub active: usize,
    /// クリップ切り取り/コピーのクリップボード（AviQtl::TimelineView::contextMenu相当）。
    /// プロジェクト横断で共有する（AviUtl本体同様、セッション切替後も貼り付け可能とする）。
    pub clipboard: Vec<ObjectDoc>,
    /// 全プロジェクト共有の直列レンダーキュー。
    pub render_queue: crate::export::RenderQueue,
}

pub type SharedAppState = Arc<Mutex<AppState>>;

impl AppState {
    pub fn new(first: ProjectSession) -> SharedAppState {
        Arc::new(Mutex::new(Self {
            sessions: vec![first],
            active: 0,
            clipboard: Vec::new(),
            render_queue: crate::export::RenderQueue::new(),
        }))
    }
}

pub fn active_world(state: &SharedAppState) -> Arc<Mutex<EcsWorld>> {
    let s = state.lock().unwrap();
    s.sessions[s.active].world.clone()
}

pub fn active_engine(state: &SharedAppState) -> Arc<Mutex<Option<RenderEngine>>> {
    let s = state.lock().unwrap();
    s.sessions[s.active].engine.clone()
}

pub fn active_audio_mixer(state: &SharedAppState) -> Arc<Mutex<AudioMixer>> {
    let s = state.lock().unwrap();
    s.sessions[s.active].audio_mixer.clone()
}

pub fn activate_session_by_dir(
    state: &SharedAppState,
    dir: &std::path::Path,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let Some(index) = s
        .sessions
        .iter()
        .position(|session| session.meta.dir == dir)
    else {
        return Err(format!(
            "プロジェクトセッションが見つかりません: {}",
            dir.display()
        ));
    };
    s.active = index;
    Ok(())
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
    s.sessions[active].dirty = true;
}

pub fn autosave_active(state: &SharedAppState) -> bool {
    let world_holder = active_world(state);
    let result = {
        let world = world_holder.lock().unwrap();
        crate::project::save_autosave_from_world(&world)
    };
    let mut s = state.lock().unwrap();
    let active = s.active;
    if result.is_ok() {
        s.sessions[active].last_autosave = Instant::now();
    }
    if let Err(err) = &result {
        eprintln!("[NeoUtl] オートセーブ失敗: {err}");
    }
    result.is_ok()
}

pub fn save_all(state: &SharedAppState) {
    let mut s = state.lock().unwrap();
    for session in &mut s.sessions {
        let world = session.world.lock().unwrap();
        if let Err(err) = crate::project::save_from_world(&world) {
            eprintln!("[NeoUtl] プロジェクト保存失敗: {err}");
        } else {
            session.dirty = false;
        }
    }
}

pub fn save_active(state: &SharedAppState) -> bool {
    let world_holder = active_world(state);
    let result = {
        let world = world_holder.lock().unwrap();
        crate::project::save_from_world(&world)
    };
    if let Err(err) = &result {
        eprintln!("[NeoUtl] プロジェクト保存失敗: {err}");
    }
    if result.is_ok() {
        let mut s = state.lock().unwrap();
        let active = s.active;
        s.sessions[active].dirty = false;
        s.sessions[active].last_autosave = Instant::now();
    }
    result.is_ok()
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

/// クリップボードへコピー/切り取り結果を格納する。
pub fn set_clipboard(state: &SharedAppState, docs: Vec<crate::document::ObjectDoc>) {
    let mut s = state.lock().unwrap();
    s.clipboard = docs;
}

/// クリップボード内容の複製を取得する（貼り付け時に消費せず複数回貼り付け可能とする）。
pub fn clipboard(state: &SharedAppState) -> Vec<crate::document::ObjectDoc> {
    state.lock().unwrap().clipboard.clone()
}
