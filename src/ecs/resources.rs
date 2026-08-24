use crate::config;
use shipyard::Unique;

#[derive(Clone, Debug, Unique)]
pub struct ProjectResource {
    pub name: String,
    pub dir: Option<std::path::PathBuf>,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
}

impl ProjectResource {
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

#[derive(Unique)]
pub struct LayerStates(pub Vec<(bool, bool)>);

impl LayerStates {
    pub fn new(count: usize) -> Self {
        Self(vec![(true, false); count])
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

pub const GRID_MODE_AUTO: i32 = 0;

#[derive(Clone, Debug)]
pub struct SceneMeta {
    pub id: i32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub total_frames: i32,
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

#[derive(Clone, Debug, Unique)]
pub struct SystemSettingsResource {
    pub autosave_enabled: bool,
    pub autosave_interval_sec: i32,
    pub theme_dark: bool,
    pub theme_id: String,
    pub easing_engine_id: String,
    pub ui_scale_percent: i32,
    pub worker_threads: i32,
    pub audio_max_block_size: i32,
    pub decode_backend: i32,
    pub default_snap: bool,
    pub magnetic_snap_range: i32,
    pub export_container: i32,
    pub export_codec: i32,
    pub check_update_on_startup: bool,
    pub crash_reporting_enabled: bool,
    pub max_group_chain_depth: i32,
}

impl From<&SystemSettingsResource> for neoutl_schema::SystemSettings {
    fn from(value: &SystemSettingsResource) -> Self {
        Self {
            autosave_enabled: value.autosave_enabled,
            autosave_interval_sec: value.autosave_interval_sec,
            theme_dark: value.theme_dark,
            theme_id: value.theme_id.clone(),
            easing_engine_id: value.easing_engine_id.clone(),
            ui_scale_percent: value.ui_scale_percent,
            worker_threads: value.worker_threads,
            audio_max_block_size: value.audio_max_block_size,
            decode_backend: value.decode_backend,
            default_snap: value.default_snap,
            magnetic_snap_range: value.magnetic_snap_range,
            export_container: value.export_container,
            export_codec: value.export_codec,
            check_update_on_startup: value.check_update_on_startup,
            crash_reporting_enabled: value.crash_reporting_enabled,
            max_group_chain_depth: value.max_group_chain_depth,
        }
    }
}

impl TryFrom<&neoutl_schema::SystemSettings> for SystemSettingsResource {
    type Error = String;

    fn try_from(value: &neoutl_schema::SystemSettings) -> Result<Self, Self::Error> {
        Ok(Self {
            autosave_enabled: value.autosave_enabled,
            autosave_interval_sec: value.autosave_interval_sec,
            theme_dark: value.theme_dark,
            theme_id: value.theme_id.clone(),
            easing_engine_id: value.easing_engine_id.clone(),
            ui_scale_percent: value.ui_scale_percent,
            worker_threads: value.worker_threads,
            audio_max_block_size: value.audio_max_block_size,
            decode_backend: value.decode_backend,
            default_snap: value.default_snap,
            magnetic_snap_range: value.magnetic_snap_range,
            export_container: value.export_container,
            export_codec: value.export_codec,
            check_update_on_startup: value.check_update_on_startup,
            crash_reporting_enabled: value.crash_reporting_enabled,
            max_group_chain_depth: value.max_group_chain_depth,
        })
    }
}

impl From<&SceneMeta> for neoutl_schema::SceneMeta {
    fn from(value: &SceneMeta) -> Self {
        Self {
            id: value.id,
            name: value.name.clone(),
            width: value.width,
            height: value.height,
            fps: value.fps,
            grid_mode: value.grid_mode,
            grid_bpm: value.grid_bpm,
            grid_offset: value.grid_offset,
            grid_interval: value.grid_interval,
            grid_subdivision: value.grid_subdivision,
            enable_snap: value.enable_snap,
            magnetic_snap_range: value.magnetic_snap_range,
        }
    }
}

impl TryFrom<&neoutl_schema::SceneMeta> for SceneMeta {
    type Error = String;

    fn try_from(value: &neoutl_schema::SceneMeta) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            name: value.name.clone(),
            width: value.width,
            height: value.height,
            fps: value.fps,
            total_frames: default_total_frames(),
            layer_states: default_layer_states(),
            grid_mode: value.grid_mode,
            grid_bpm: value.grid_bpm,
            grid_offset: value.grid_offset,
            grid_interval: value.grid_interval,
            grid_subdivision: value.grid_subdivision,
            enable_snap: value.enable_snap,
            magnetic_snap_range: value.magnetic_snap_range,
        })
    }
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
            easing_engine_id: config::SYSTEM_DEFAULT_EASING_ENGINE_ID.to_string(),
            ui_scale_percent: config::SYSTEM_DEFAULT_UI_SCALE_PERCENT,
            worker_threads: config::SYSTEM_DEFAULT_WORKER_THREADS,
            audio_max_block_size: config::SYSTEM_DEFAULT_AUDIO_MAX_BLOCK_SIZE,
            decode_backend: config::SYSTEM_DEFAULT_DECODE_BACKEND,
            default_snap: config::SYSTEM_DEFAULT_DEFAULT_SNAP,
            magnetic_snap_range: config::SYSTEM_DEFAULT_MAGNETIC_SNAP_RANGE,
            export_container: config::SYSTEM_DEFAULT_EXPORT_CONTAINER,
            export_codec: config::SYSTEM_DEFAULT_EXPORT_CODEC,
            check_update_on_startup: config::SYSTEM_DEFAULT_CHECK_UPDATE_ON_STARTUP,
            crash_reporting_enabled: config::SYSTEM_DEFAULT_CRASH_REPORTING_ENABLED,
            max_group_chain_depth: config::SYSTEM_DEFAULT_MAX_GROUP_CHAIN_DEPTH,
        }
    }
}
