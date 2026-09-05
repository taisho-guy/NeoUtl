pub mod audio_plugins;
pub mod components;
pub mod effects;
pub(crate) mod object_query_views;
pub mod object_schema;
pub mod resources;
pub mod systems;
pub mod transform;
pub mod types;
mod world;

use resources::SceneMeta;

fn resolve_stable_id(kind_id: u32, object_id: usize) -> String {
    match crate::objects::loader::by_kind_id(kind_id) {
        Some(plugin) => plugin.stable_id.clone(),
        None => {
            eprintln!(
                "{}",
                t!(
                    "[NeoUtl] オブジェクト %{arg0} の kind_id=%{arg1} を stable_id へ解決不能、空値で保存",
                    arg0 = format!("{}", object_id),
                    arg1 = format!("{}", kind_id)
                )
            );
            String::new()
        }
    }
}

#[derive(Clone, Debug)]
pub struct TimelineData {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub layer: i32,
    pub media_path: Option<std::path::PathBuf>,
    pub media_trim_in_frame: i64,
    pub group_layer_count_down: i32,
    pub group_layer_count_up: i32,
    pub clip_layer_count_down: i32,
    pub clip_layer_count_up: i32,
}

#[derive(Clone, Debug)]
pub struct SceneSettings {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub grid_mode: i32,
    pub grid_bpm: f32,
    pub grid_offset: f32,
    pub grid_interval: i32,
    pub grid_subdivision: i32,
    pub enable_snap: bool,
    pub magnetic_snap_range: i32,
}

impl From<&SceneMeta> for SceneSettings {
    fn from(s: &SceneMeta) -> Self {
        Self {
            name: s.name.clone(),
            width: s.width,
            height: s.height,
            fps: s.fps,
            grid_mode: s.grid_mode,
            grid_bpm: s.grid_bpm,
            grid_offset: s.grid_offset,
            grid_interval: s.grid_interval,
            grid_subdivision: s.grid_subdivision,
            enable_snap: s.enable_snap,
            magnetic_snap_range: s.magnetic_snap_range,
        }
    }
}

pub struct EcsWorld {
    pub world: shipyard::World,
    selected_ids: std::collections::HashSet<usize>,
    revision: u64,
}
