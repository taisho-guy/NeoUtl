use crate::ecs::components::{TextAlign, TextContent};
use wgpu_text::glyph_brush::{HorizontalAlign, Layout, Section, Text};

pub fn build_section<'a>(
    content: &'a TextContent,
    render_width: u32,
    render_height: u32,
) -> Section<'a> {
    let color = content.color;
    let position = (
        content.x * render_width as f32,
        content.y * render_height as f32,
    );
    let h_align = match content.align {
        TextAlign::Left => HorizontalAlign::Left,
        TextAlign::Center => HorizontalAlign::Center,
        TextAlign::Right => HorizontalAlign::Right,
    };
    Section::default()
        .add_text(
            Text::new(&content.text)
                .with_color(color)
                .with_scale(content.font_size),
        )
        .with_screen_position(position)
        .with_layout(Layout::default_single_line().h_align(h_align))
}
