use wgpu_text::glyph_brush::ab_glyph::{Font, FontArc, ScaleFont};
use wgpu_text::glyph_brush::{HorizontalAlign, Layout, Section, Text, VerticalAlign};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

pub struct TextLayout {
    pub lines: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub line_height_px: f32,
}

pub fn layout(font: &FontArc, text: &str, font_size: f32, line_height: f32) -> TextLayout {
    let scaled = font.as_scaled(font_size);
    let line_height_px = (scaled.ascent() - scaled.descent()) * line_height.max(0.01);
    let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let width: f32 = lines
        .iter()
        .map(|line| {
            line.chars()
                .map(|c| scaled.h_advance(font.glyph_id(c)))
                .sum::<f32>()
        })
        .fold(0.0_f32, f32::max);
    let height = line_height_px * lines.len().max(1) as f32;
    TextLayout {
        width: width.ceil().max(1.0) as u32,
        height: height.ceil().max(1.0) as u32,
        line_height_px,
        lines,
    }
}

pub fn build_sections<'a>(
    color: [f32; 4],
    font_size: f32,
    align: HAlign,
    text_layout: &'a TextLayout,
    tex_width: u32,
    tex_height: u32,
) -> Vec<Section<'a>> {
    let h_align = match align {
        HAlign::Left => HorizontalAlign::Left,
        HAlign::Center => HorizontalAlign::Center,
        HAlign::Right => HorizontalAlign::Right,
    };
    let x = match h_align {
        HorizontalAlign::Left => 0.0,
        HorizontalAlign::Center => tex_width as f32 / 2.0,
        HorizontalAlign::Right => tex_width as f32,
    };
    text_layout
        .lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let y = row as f32 * text_layout.line_height_px;
            Section::default()
                .add_text(Text::new(line).with_color(color).with_scale(font_size))
                .with_screen_position((x, y))
                .with_bounds((tex_width as f32, tex_height as f32))
                .with_layout(
                    Layout::default_single_line()
                        .h_align(h_align)
                        .v_align(VerticalAlign::Top),
                )
        })
        .collect()
}
