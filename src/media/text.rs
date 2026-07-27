use crate::ecs::components::{TextAlign, TextContent};
use wgpu_text::glyph_brush::{HorizontalAlign, Layout, Section, Text};

/// world_x/world_yはTransform由来のワールドピクセル座標（中心原点、+Yが上）。
/// render_width/render_heightは出力解像度。スクリーン座標（左上原点、+Yが下）へ変換する。
pub fn build_section(
    content: &TextContent,
    world_x: f32,
    world_y: f32,
    render_width: u32,
    render_height: u32,
) -> Section<'_> {
    let color = content.color;
    let position = (
        render_width as f32 / 2.0 + world_x,
        render_height as f32 / 2.0 - world_y,
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
