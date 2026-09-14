use super::group_control::bool_to_f32;
use super::param_access::ParamAccess;
use serde::{Deserialize, Serialize};
use shipyard::Component;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Component, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub font_family_stack: Vec<String>,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    pub line_height: f32,
    pub outline_width: f32,
    pub outline_color: [f32; 4],
}

impl From<&TextContent> for neoutl_schema::TextContent {
    fn from(value: &TextContent) -> Self {
        Self {
            text: value.text.clone(),
            font_size: value.font_size,
            color: value.color.to_vec(),
            font_family_stack: value.font_family_stack.clone(),
            bold: value.bold,
            italic: value.italic,
            align: match value.align {
                TextAlign::Left => neoutl_schema::TextAlign::Left as i32,
                TextAlign::Center => neoutl_schema::TextAlign::Center as i32,
                TextAlign::Right => neoutl_schema::TextAlign::Right as i32,
            },
            line_height: value.line_height,
            outline_width: value.outline_width,
            outline_color: value.outline_color.to_vec(),
        }
    }
}

impl TryFrom<&neoutl_schema::TextContent> for TextContent {
    type Error = String;

    fn try_from(value: &neoutl_schema::TextContent) -> Result<Self, Self::Error> {
        let mut color = [0.0; 4];
        for (idx, v) in value.color.iter().take(4).enumerate() {
            color[idx] = *v;
        }
        let mut outline_color = [0.0; 4];
        for (idx, v) in value.outline_color.iter().take(4).enumerate() {
            outline_color[idx] = *v;
        }
        Ok(Self {
            text: value.text.clone(),
            font_size: value.font_size,
            color,
            font_family_stack: value.font_family_stack.clone(),
            bold: value.bold,
            italic: value.italic,
            align: match value.align() {
                neoutl_schema::TextAlign::Left => TextAlign::Left,
                neoutl_schema::TextAlign::Center => TextAlign::Center,
                neoutl_schema::TextAlign::Right => TextAlign::Right,
            },
            line_height: value.line_height,
            outline_width: value.outline_width,
            outline_color,
        })
    }
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: "New Text".to_owned(),
            font_size: 48.0,
            color: [1.0, 1.0, 1.0, 1.0],
            font_family_stack: vec![String::new()],
            bold: false,
            italic: false,
            align: TextAlign::Left,
            line_height: 1.2,
            outline_width: 0.0,
            outline_color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl ParamAccess for TextContent {
    fn get_param(&self, key: &str) -> Option<f32> {
        Some(match key {
            "font_size" => self.font_size,
            "bold" => bool_to_f32(self.bold),
            "italic" => bool_to_f32(self.italic),
            "align" => self.align as u8 as f32,
            "line_height" => self.line_height,
            "outline_width" => self.outline_width,
            "outline_r" => self.outline_color[0],
            "outline_g" => self.outline_color[1],
            "outline_b" => self.outline_color[2],
            "outline_a" => self.outline_color[3],
            _ => return None,
        })
    }
    fn set_param(&mut self, key: &str, value: f32) -> bool {
        match key {
            "font_size" => self.font_size = value,
            "bold" => self.bold = value > 0.5,
            "italic" => self.italic = value > 0.5,
            "align" => {
                self.align = match value.round() as i32 {
                    1 => TextAlign::Center,
                    2 => TextAlign::Right,
                    _ => TextAlign::Left,
                }
            }
            "line_height" => self.line_height = value,
            "outline_width" => self.outline_width = value,
            "outline_r" => self.outline_color[0] = value,
            "outline_g" => self.outline_color[1] = value,
            "outline_b" => self.outline_color[2] = value,
            "outline_a" => self.outline_color[3] = value,
            _ => return false,
        }
        true
    }
}
