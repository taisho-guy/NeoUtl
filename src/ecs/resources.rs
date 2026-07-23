use crate::config;
use serde::{Deserialize, Serialize};
use shipyard::Unique;

/// 動画プロジェクト全体の設定（FPS・解像度等）
#[derive(Clone, Debug, Unique)]
pub struct ProjectResource {
    /// プロジェクト名
    pub name: String,
    /// プロジェクトディレクトリ（未保存時はNone）
    pub dir: Option<std::path::PathBuf>,
    /// フレームレート（frames per second）
    pub fps: u32,
    /// 出力幅（ピクセル）
    pub width: u32,
    /// 出力高さ（ピクセル）
    pub height: u32,
    /// 音声サンプリングレート（Hz）
    pub audio_sample_rate: u32,
    /// 音声チャンネル数（1=モノラル, 2=ステレオ）
    pub audio_channels: u32,
}

impl ProjectResource {
    /// 新規プロジェクト・シーンの既定解像度。Camera::for_resolution()の
    /// ブートストラップ値と共有し、値の重複を避ける。
    pub const DEFAULT_WIDTH: u32 = config::PROJECT_DEFAULT_WIDTH;
    pub const DEFAULT_HEIGHT: u32 = config::PROJECT_DEFAULT_HEIGHT;

    pub fn new() -> Self {
        Self {
            name: String::new(),
            dir: None,
            fps: config::PROJECT_DEFAULT_FPS,
            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
            audio_sample_rate: config::PROJECT_DEFAULT_AUDIO_SAMPLE_RATE,
            audio_channels: config::PROJECT_DEFAULT_AUDIO_CHANNELS,
        }
    }
}

pub const DEFAULT_LAYER_COUNT: usize = config::DEFAULT_LAYER_COUNT;

fn default_total_frames() -> i32 {
    config::DEFAULT_TOTAL_FRAMES
}

fn default_layer_states() -> Vec<(bool, bool)> {
    vec![(true, false); DEFAULT_LAYER_COUNT]
}

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
            total_frames: config::DEFAULT_TOTAL_FRAMES,
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

/// グリッドモード（AviQtl::UI::SceneData::gridMode相当）。
/// 0: Auto（秒/フレーム）, 1: BPM（音楽）, 2: Frame（フレーム数固定）
pub const GRID_MODE_AUTO: i32 = 0;
pub const GRID_MODE_BPM: i32 = 1;
pub const GRID_MODE_FRAME: i32 = 2;

/// シーン単体の設定（AviQtl::UI::SceneData 相当。グリッド・スナップはシーン単位で保持する）。
/// `Serialize`/`Deserialize`をプロジェクトファイルへの直接永続化に用いる。
/// `total_frames`・`layer_states`はランタイム状態のため保存対象外とし、
/// 復元時は既定値（`default_total_frames`・`default_layer_states`）で補完する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneMeta {
    pub id: i32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    #[serde(skip, default = "default_total_frames")]
    pub total_frames: i32,
    #[serde(skip, default = "default_layer_states")]
    pub layer_states: Vec<(bool, bool)>,

    pub grid_mode: i32,
    pub grid_bpm: f32,
    pub grid_offset: f32,
    pub grid_interval: i32,
    pub grid_subdivision: i32,
    pub enable_snap: bool,
    pub magnetic_snap_range: i32,
}

impl SceneMeta {
    pub fn new(id: i32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            width: ProjectResource::DEFAULT_WIDTH,
            height: ProjectResource::DEFAULT_HEIGHT,
            fps: config::PROJECT_DEFAULT_FPS,
            total_frames: default_total_frames(),
            layer_states: default_layer_states(),
            grid_mode: GRID_MODE_AUTO,
            grid_bpm: config::SCENE_DEFAULT_GRID_BPM,
            grid_offset: config::SCENE_DEFAULT_GRID_OFFSET,
            grid_interval: config::SCENE_DEFAULT_GRID_INTERVAL,
            grid_subdivision: config::SCENE_DEFAULT_GRID_SUBDIVISION,
            enable_snap: config::SCENE_DEFAULT_ENABLE_SNAP,
            magnetic_snap_range: config::SCENE_DEFAULT_MAGNETIC_SNAP_RANGE,
        }
    }

    /// システム設定の既定値（新規シーン用）を反映したメタ情報を生成する。
    /// UI側（scene_settings.rs::open_for_create）はこの結果のみを初期値として使い、
    /// グリッド既定値を独自に書き直さない。
    pub fn new_with_defaults(
        id: i32,
        name: impl Into<String>,
        default_snap: bool,
        magnetic_snap_range: i32,
    ) -> Self {
        let mut meta = Self::new(id, name);
        meta.enable_snap = default_snap;
        meta.magnetic_snap_range = magnetic_snap_range;
        meta
    }

    /// グリッド間隔・スナップ許容範囲に基づき、フレーム番号を最寄りのグリッド線へ吸着させる。
    /// `enable_snap`がfalse、または許容範囲外の場合は入力値をそのまま返す。
    pub fn snap_frame(&self, frame: i32) -> i32 {
        if !self.enable_snap || self.grid_interval <= 0 {
            return frame;
        }
        let interval = self.grid_interval;
        let nearest = ((frame as f32 / interval as f32).round() as i32) * interval;
        if (nearest - frame).abs() <= self.magnetic_snap_range {
            nearest
        } else {
            frame
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

/// システム全体の設定（AviQtl::Core::SettingsManager 相当）
#[derive(Clone, Debug, Unique, Serialize, Deserialize)]
pub struct SystemSettingsResource {
    pub autosave_enabled: bool,
    pub autosave_interval_sec: i32,
    pub theme_dark: bool,
    /// 選択中テーマのstable_id。空文字は未選択（theme_darkの明暗2値へフォールバック）
    #[serde(default)]
    pub theme_id: String,
    pub ui_scale_percent: i32,
    /// 0: 自動（論理コア数に追従） / 1以上: デコードワーカーの上限並列数
    pub worker_threads: i32,
    pub audio_max_block_size: i32,
    /// 0: 自動 (GPU優先, 失敗時CPU) / 1: GPU固定 / 2: CPU固定
    pub decode_backend: i32,
    /// 新規シーン作成時に引き継ぐスナップ既定値
    pub default_snap: bool,
    pub magnetic_snap_range: i32,
    /// 0: MP4 / 1: MOV / 2: MKV
    pub export_container: i32,
    /// 0: H.264 / 1: HEVC / 2: AV1
    pub export_codec: i32,
}

impl Default for SystemSettingsResource {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSettingsResource {
    pub fn new() -> Self {
        Self {
            autosave_enabled: config::SYSTEM_DEFAULT_AUTOSAVE_ENABLED,
            autosave_interval_sec: config::SYSTEM_DEFAULT_AUTOSAVE_INTERVAL_SEC,
            theme_dark: config::SYSTEM_DEFAULT_THEME_DARK,
            theme_id: config::SYSTEM_DEFAULT_THEME_ID.to_string(),
            ui_scale_percent: config::SYSTEM_DEFAULT_UI_SCALE_PERCENT,
            worker_threads: config::SYSTEM_DEFAULT_WORKER_THREADS,
            audio_max_block_size: config::SYSTEM_DEFAULT_AUDIO_MAX_BLOCK_SIZE,
            decode_backend: config::SYSTEM_DEFAULT_DECODE_BACKEND,
            default_snap: config::SYSTEM_DEFAULT_DEFAULT_SNAP,
            magnetic_snap_range: config::SYSTEM_DEFAULT_MAGNETIC_SNAP_RANGE,
            export_container: config::SYSTEM_DEFAULT_EXPORT_CONTAINER,
            export_codec: config::SYSTEM_DEFAULT_EXPORT_CODEC,
        }
    }
}
