// src/ecs/components.rs
use shipyard::Component;

#[derive(Clone, Copy, Debug, Component)]
pub struct TimeRange {
    pub start_frame: i32,
    pub end_frame: i32,
}

#[derive(Clone, Copy, Debug, Component)]
pub struct ObjectId(pub usize);

#[derive(Clone, Copy, Debug, Component)]
pub struct KindId(pub u32);

#[derive(Clone, Copy, Debug, Component)]
pub struct Layer(pub i32);

#[derive(Clone, Copy, Debug, Component)]
pub struct SceneId(pub i32);

#[derive(Clone, Copy, Debug, Component)]
pub struct AudioParams {
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            mute: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Component)]
pub struct TextContent {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [f32; 4],
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub line_height: f32,
    pub outline_width: f32,
    pub outline_color: [f32; 4],
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_owned(),
            x: 0.05,
            y: 0.05,
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            font_family: String::new(),
            bold: false,
            italic: false,
            align: TextAlign::Left,
            line_height: 1.2,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// 図形種別。sides==4はRect、sides>=8はEllipse近似として扱う（現行UI上のプリセット分岐）。
#[derive(Clone, Copy, Debug, Component)]
pub struct ShapeParams {
    pub sides: u32,
    pub fill_color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub extrude_depth: f32,
}

impl Default for ShapeParams {
    fn default() -> Self {
        Self {
            sides: 4,
            fill_color: [1.0, 1.0, 1.0, 1.0],
            stroke_color: [0.0, 0.0, 0.0, 0.0],
            stroke_width: 0.0,
            extrude_depth: 0.0,
        }
    }
}
