// src/ecs/resources.rs
use shipyard::Unique;

/// 動画プロジェクト全体の設定（FPS・解像度等）
#[derive(Clone, Debug, Unique)]
pub struct ProjectResource {
    /// フレームレート（frames per second）
    pub fps: u32,
    /// 出力幅（ピクセル）
    pub width: u32,
    /// 出力高さ（ピクセル）
    pub height: u32,
}

impl ProjectResource {
    pub fn new() -> Self {
        Self {
            fps: 30,
            width: 1920,
            height: 1080,
        }
    }
}

pub const DEFAULT_LAYER_COUNT: usize = 128;

/// タイムライン状態（再生ヘッド・フレーム総数・ズーム率など）
#[derive(Unique)]
pub struct TimelineResource {
    pub current_frame: i32,
    pub total_frames: i32,
    pub next_id: usize,
    pub zoom_scale: f32,
    pub layer_count: i32,
}

impl TimelineResource {
    pub fn new() -> Self {
        Self {
            current_frame: 0,
            total_frames: 300,
            next_id: 1,
            zoom_scale: 1.0,
            layer_count: DEFAULT_LAYER_COUNT as i32,
        }
    }
}

/// 各レイヤーの表示・ロック状態
#[derive(Unique)]
pub struct LayerStates(pub Vec<(bool, bool)>);

impl LayerStates {
    pub fn new(count: usize) -> Self {
        Self(vec![(true, false); count])
    }

    pub fn visible(&self, layer: usize) -> bool {
        self.0.get(layer).map(|s| s.0).unwrap_or(true)
    }

    pub fn locked(&self, layer: usize) -> bool {
        self.0.get(layer).map(|s| s.1).unwrap_or(false)
    }

    pub fn set_visible(&mut self, layer: usize, v: bool) {
        if let Some(s) = self.0.get_mut(layer) {
            s.0 = v;
        }
    }

    pub fn set_locked(&mut self, layer: usize, v: bool) {
        if let Some(s) = self.0.get_mut(layer) {
            s.1 = v;
        }
    }
}

/// シーン単体の設定（AviQtl::Core::SceneSettings 相当）
#[derive(Clone, Debug)]
pub struct SceneMeta {
    pub id: i32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub total_frames: i32,
    pub layer_states: Vec<(bool, bool)>,
}

impl SceneMeta {
    pub fn new(id: i32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            width: 1920,
            height: 1080,
            fps: 30,
            total_frames: 300,
            layer_states: vec![(true, false); DEFAULT_LAYER_COUNT],
        }
    }
}

/// プロジェクト内の全シーンとアクティブシーン（AviQtl::Core::DocumentModel 相当）
#[derive(Unique)]
pub struct SceneResource {
    pub scenes: Vec<SceneMeta>,
    pub active_scene: i32,
    pub next_scene_id: i32,
}

impl SceneResource {
    pub fn new() -> Self {
        Self {
            scenes: vec![SceneMeta::new(0, "Scene 1")],
            active_scene: 0,
            next_scene_id: 1,
        }
    }

    pub fn find(&self, id: i32) -> Option<&SceneMeta> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn find_mut(&mut self, id: i32) -> Option<&mut SceneMeta> {
        self.scenes.iter_mut().find(|s| s.id == id)
    }
}
