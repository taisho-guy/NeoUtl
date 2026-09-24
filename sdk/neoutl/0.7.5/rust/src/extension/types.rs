use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub url: Option<String>,
}

impl ExtensionMetadata {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: "0.1.0".to_owned(),
            author: "Unknown".to_owned(),
            description: String::new(),
            url: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuLocation {
    MenuBar(MenuBarSection),
    Import,
    Export,
    TimelineLayerContext,
    TimelineClipContext,
    PropertyItemContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuBarSection {
    File,
    Edit,
    View,
    Tools,
    Plugins,
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct MenuItemDescriptor {
    pub id: String,
    pub location: MenuLocation,
    pub label: String,
    pub shortcut_hint: Option<String>,
    pub command_id: String,
    pub enabled: bool,
    pub checked: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelPlacement {
    FloatingWindow,
    Dockable,
    Sidebar,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PanelDescriptor {
    pub id: String,
    pub title: String,
    pub default_size: [f32; 2],
    pub default_open: bool,
    pub placement: PanelPlacement,
}

pub struct PreviewOverlayContext<'a> {
    pub painter: &'a egui::Painter,
    pub viewport_rect: egui::Rect,
    pub scene_resolution: [u32; 2],
    pub current_frame: i32,
    pub fps: u32,
    pub is_playing: bool,
    pub selected_object_ids: &'a [usize],
}

impl<'a> PreviewOverlayContext<'a> {
    pub fn scene_to_screen_pos(&self, scene_x: f32, scene_y: f32) -> egui::Pos2 {
        let sw = self.scene_resolution[0] as f32;
        let sh = self.scene_resolution[1] as f32;
        if sw == 0.0 || sh == 0.0 {
            return self.viewport_rect.center();
        }
        let norm_x = (scene_x + sw * 0.5) / sw;
        let norm_y = (scene_y + sh * 0.5) / sh;
        egui::pos2(
            self.viewport_rect.min.x + norm_x * self.viewport_rect.width(),
            self.viewport_rect.min.y + norm_y * self.viewport_rect.height(),
        )
    }

    pub fn screen_to_scene_pos(&self, screen_pos: egui::Pos2) -> (f32, f32) {
        let sw = self.scene_resolution[0] as f32;
        let sh = self.scene_resolution[1] as f32;
        if self.viewport_rect.width() == 0.0 || self.viewport_rect.height() == 0.0 {
            return (0.0, 0.0);
        }
        let norm_x = (screen_pos.x - self.viewport_rect.min.x) / self.viewport_rect.width();
        let norm_y = (screen_pos.y - self.viewport_rect.min.y) / self.viewport_rect.height();
        (norm_x * sw - sw * 0.5, norm_y * sh - sh * 0.5)
    }
}

pub struct TimelineOverlayContext<'a> {
    pub painter: &'a egui::Painter,
    pub timeline_rect: egui::Rect,
    pub visible_frame_range: (i32, i32),
    pub current_frame: i32,
    pub fps: u32,
    pub bpm: Option<f32>,
    pub selected_object_ids: &'a [usize],
    pub frame_to_x: &'a dyn Fn(i32) -> f32,
    pub layer_to_y: &'a dyn Fn(i32) -> f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExtensionEvent {
    ProjectLoaded {
        project_name: String,
        dir: String,
    },
    ProjectSaved {
        project_name: String,
        dir: String,
    },
    ProjectClosed,
    FrameChanged {
        frame: i32,
        total_frames: i32,
        scene_id: i32,
    },
    PlaybackChanged {
        is_playing: bool,
        frame: i32,
    },
    SceneChanged {
        scene_id: i32,
        scene_name: String,
    },
    SelectionChanged {
        selected_ids: Vec<usize>,
    },
    ObjectAdded {
        id: usize,
        layer: i32,
        start: i32,
        duration: i32,
    },
    ObjectModified {
        id: usize,
    },
    ObjectDeleted {
        id: usize,
    },
    FileDropped {
        path: String,
        layer: i32,
        frame: i32,
    },
    Custom {
        sender: String,
        name: String,
        payload: serde_json::Value,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectStorage {
    strings: HashMap<String, String>,
    json_blobs: HashMap<String, serde_json::Value>,
    bytes: HashMap<String, Vec<u8>>,
}

impl ProjectStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }

    pub fn set_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.strings.insert(key.into(), value.into());
    }

    pub fn get_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.json_blobs
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn set_json<T: Serialize>(
        &mut self,
        key: impl Into<String>,
        value: &T,
    ) -> Result<(), serde_json::Error> {
        let v = serde_json::to_value(value)?;
        self.json_blobs.insert(key.into(), v);
        Ok(())
    }

    pub fn get_bytes(&self, key: &str) -> Option<&[u8]> {
        self.bytes.get(key).map(|v| v.as_slice())
    }

    pub fn set_bytes(&mut self, key: impl Into<String>, bytes: &[u8]) {
        self.bytes.insert(key.into(), bytes.to_vec());
    }

    pub fn remove(&mut self, key: &str) {
        self.strings.remove(key);
        self.json_blobs.remove(key);
        self.bytes.remove(key);
    }

    pub fn clear(&mut self) {
        self.strings.clear();
        self.json_blobs.clear();
        self.bytes.clear();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub id: usize,
    pub scene_id: i32,
    pub kind_stable_id: String,
    pub name: String,
    pub layer: i32,
    pub start_frame: i32,
    pub duration: i32,
    pub end_frame: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectInfo {
    pub index: usize,
    pub effect_id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub dir: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub audio_sample_rate: u32,
    pub audio_channels: u32,
    pub active_scene_id: i32,
}

pub trait EditSession {
    fn project_info(&self) -> ProjectInfo;
    fn cursor_frame(&self) -> i32;
    fn total_frames(&self) -> i32;
    fn selected_object_ids(&self) -> Vec<usize>;
    fn find_objects_at(&self, layer: i32, frame: i32) -> Vec<ObjectInfo>;
    fn get_object(&self, id: usize) -> Option<ObjectInfo>;
    fn get_object_effects(&self, object_id: usize) -> Vec<EffectInfo>;
    fn get_param_float(
        &self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
    ) -> Option<f32>;
    fn get_param_string(
        &self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
    ) -> Option<String>;

    fn set_cursor_frame(&mut self, frame: i32);
    fn select_object(&mut self, id: usize, clear_others: bool);
    fn clear_selection(&mut self);

    fn create_object(
        &mut self,
        stable_id: &str,
        layer: i32,
        start_frame: i32,
        duration: i32,
    ) -> Result<usize, String>;

    fn create_text_object(
        &mut self,
        text: &str,
        layer: i32,
        start_frame: i32,
        duration: i32,
    ) -> Result<usize, String>;

    fn create_media_object(
        &mut self,
        file_path: &str,
        layer: i32,
        start_frame: i32,
        duration: Option<i32>,
    ) -> Result<usize, String>;

    fn move_object(
        &mut self,
        id: usize,
        new_layer: i32,
        new_start_frame: i32,
    ) -> Result<(), String>;
    fn resize_object(&mut self, id: usize, new_duration: i32) -> Result<(), String>;
    fn delete_object(&mut self, id: usize) -> Result<(), String>;
    fn duplicate_object(&mut self, id: usize) -> Result<usize, String>;

    fn add_effect(&mut self, object_id: usize, effect_id: &str) -> Result<usize, String>;
    fn remove_effect(&mut self, object_id: usize, effect_idx: usize) -> Result<(), String>;
    fn set_effect_enabled(
        &mut self,
        object_id: usize,
        effect_idx: usize,
        enabled: bool,
    ) -> Result<(), String>;

    fn set_param_float(
        &mut self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
        value: f32,
    ) -> Result<(), String>;

    fn set_param_string(
        &mut self,
        object_id: usize,
        effect_idx: Option<usize>,
        key: &str,
        value: &str,
    ) -> Result<(), String>;

    fn create_scene(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<i32, String>;
    fn switch_scene(&mut self, scene_id: i32) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct CommandDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_shortcut: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FileDropDescriptor {
    pub name: String,
    pub file_extensions: Vec<String>,
}

pub struct ExtensionContext<'a> {
    pub plugin_id: &'a str,
    pub storage: &'a mut ProjectStorage,
    pub edit_session: Option<&'a mut dyn EditSession>,
    pub registered_panels: &'a mut Vec<PanelDescriptor>,
    pub registered_menus: &'a mut Vec<MenuItemDescriptor>,
    pub registered_commands: &'a mut Vec<CommandDescriptor>,
    pub registered_file_drops: &'a mut Vec<FileDropDescriptor>,
    pub toast_messages: &'a mut Vec<String>,
    pub outgoing_events: &'a mut Vec<ExtensionEvent>,
}

impl<'a> ExtensionContext<'a> {
    pub fn register_panel(&mut self, descriptor: PanelDescriptor) {
        self.registered_panels.push(descriptor);
    }

    pub fn register_menu(&mut self, item: MenuItemDescriptor) {
        self.registered_menus.push(item);
    }

    pub fn register_command(&mut self, descriptor: CommandDescriptor) {
        self.registered_commands.push(descriptor);
    }

    pub fn register_file_drop(&mut self, descriptor: FileDropDescriptor) {
        self.registered_file_drops.push(descriptor);
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toast_messages.push(message.into());
    }

    pub fn broadcast_event(&mut self, name: impl Into<String>, payload: serde_json::Value) {
        self.outgoing_events.push(ExtensionEvent::Custom {
            sender: self.plugin_id.to_owned(),
            name: name.into(),
            payload,
        });
    }
}

pub trait ExtensionPlugin: Send + Sync {
    fn metadata(&self) -> ExtensionMetadata;

    fn init(&mut self, ctx: &mut ExtensionContext) -> Result<(), String>;

    #[allow(unused_variables)]
    fn ui(&mut self, panel_id: &str, ui: &mut egui::Ui, ctx: &mut ExtensionContext) {}

    #[allow(unused_variables)]
    fn on_event(&mut self, event: &ExtensionEvent, ctx: &mut ExtensionContext) {}

    #[allow(unused_variables)]
    fn on_command(&mut self, command_id: &str, ctx: &mut ExtensionContext) -> Result<(), String> {
        Ok(())
    }

    #[allow(unused_variables)]
    fn on_file_drop(
        &mut self,
        path: &str,
        layer: i32,
        frame: i32,
        ctx: &mut ExtensionContext,
    ) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn draw_preview_overlay(
        &mut self,
        overlay: &mut PreviewOverlayContext,
        ctx: &mut ExtensionContext,
    ) {
    }

    #[allow(unused_variables)]
    fn draw_timeline_overlay(
        &mut self,
        overlay: &mut TimelineOverlayContext,
        ctx: &mut ExtensionContext,
    ) {
    }

    #[allow(unused_variables)]
    fn shutdown(&mut self, ctx: &mut ExtensionContext) {}
}
