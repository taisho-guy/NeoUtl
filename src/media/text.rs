use crate::ecs::components::{TextAlign, TextContent};
use wgpu_text::glyph_brush::ab_glyph::{Font, FontArc, ScaleFont};
use wgpu_text::glyph_brush::{HorizontalAlign, Layout, Section, Text, VerticalAlign};

pub fn measure(font: &FontArc, content: &TextContent) -> (u32, u32) {
    let scaled = font.as_scaled(content.font_size);
    let width: f32 = content
        .text
        .chars()
        .map(|c| scaled.h_advance(font.glyph_id(c)))
        .sum();
    let height = scaled.ascent() - scaled.descent();
    (width.ceil().max(1.0) as u32, height.ceil().max(1.0) as u32)
}

pub fn build_section(content: &TextContent, tex_width: u32, tex_height: u32) -> Section<'_> {
    let h_align = match content.align {
        TextAlign::Left => HorizontalAlign::Left,
        TextAlign::Center => HorizontalAlign::Center,
        TextAlign::Right => HorizontalAlign::Right,
    };
    let x = match h_align {
        HorizontalAlign::Left => 0.0,
        HorizontalAlign::Center => tex_width as f32 / 2.0,
        HorizontalAlign::Right => tex_width as f32,
    };
    Section::default()
        .add_text(
            Text::new(&content.text)
                .with_color(content.color)
                .with_scale(content.font_size),
        )
        .with_screen_position((x, 0.0))
        .with_bounds((tex_width as f32, tex_height as f32))
        .with_layout(
            Layout::default_single_line()
                .h_align(h_align)
                .v_align(VerticalAlign::Top),
        )
}
