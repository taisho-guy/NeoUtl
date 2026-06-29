// src/ecs/components.rs

#[derive(Clone, Copy, Debug)]
pub struct TimeRange {
    pub start_frame: i32,
    pub end_frame: i32,
}

#[derive(Clone, Debug)]
pub struct TextContent {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [f32; 4],
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_owned(),
            x: 0.05,
            y: 0.05,
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}
