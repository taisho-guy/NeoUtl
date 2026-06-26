// src/ecs/components.rs
pub use crate::objects::RenderKind;

#[derive(Clone, Copy, Debug)]
pub struct TimeRange {
    pub start_frame: i32,
    pub end_frame: i32,
}

/// テキストオブジェクトの内容と表示パラメータ
#[derive(Clone, Debug)]
pub struct TextContent {
    /// 表示するテキスト
    pub text: String,
    /// 画面左端からの相対位置（0.0 〜 1.0）
    pub x: f32,
    /// 画面上端からの相対位置（0.0 〜 1.0）
    pub y: f32,
    /// フォントサイズ（ピクセル）
    pub font_size: f32,
    /// 文字色 [R, G, B, A]（各 0.0 〜 1.0）
    pub color: [f32; 4],
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_string(),
            x: 0.05,
            y: 0.05,
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}
