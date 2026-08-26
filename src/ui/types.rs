use egui::TextureId;

#[derive(Clone, Debug, Default)]
pub struct TimelineObject {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub kind_known: bool,
    pub layer: i32,
    pub label: String,
    pub selected: bool,
    pub keyframe_frames: Vec<i32>,
    pub waveform: Option<TextureId>,
    pub has_waveform: bool,
    pub waveform_origin_frame: i32,
    pub waveform_duration_frames: i32,
    pub group_layer_count_down: i32,
    pub group_layer_count_up: i32,
    pub clip_layer_count_down: i32,
    pub clip_layer_count_up: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ObjectKindItem {
    pub kind: i32,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayerState {
    pub visible: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CatalogRow {
    pub id: String,
    pub name: String,
    pub category: String,
}

#[derive(Clone, Debug, Default)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: i32,
    pub kind: i32,
    pub enabled: bool,
    pub icon: String,
    pub checked: Option<bool>,
    pub submenu: Vec<ContextMenuItem>,
}

#[derive(Clone, Debug, Default)]
pub struct SceneTabItem {
    pub id: i32,
    pub name: String,
    pub active: bool,
}
