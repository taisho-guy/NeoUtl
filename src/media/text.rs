use crate::ecs::components::{TextAlign, TextContent};
use wgpu_text::glyph_brush::ab_glyph::{Font, FontArc, ScaleFont};
use wgpu_text::glyph_brush::{HorizontalAlign, Layout, Section, Text, VerticalAlign};

/// 1行分のテキストがちょうど収まる矩形寸法（ピクセル、パディングなし）を
/// フォントメトリクスから直接算出する。高さ=ascent-descent、幅=各字形の
/// 水平アドバンス合計（カーニング非考慮の近似、改行非対応）。
/// この寸法がテキスト専用オフスクリーンテクスチャの実寸となり、
/// renderer::pipeline側でTransform.scale_x/yとの合成（UNIT_SIZE_PX基準の
/// rescale）に用いられる。
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

/// テキスト専用オフスクリーンテクスチャ（measure()と同一寸法、tex_width/tex_height）へ
/// 描画するSection。原点(0,0)を左上としテクスチャ全域を占める。
/// テクスチャ自体はTransform経由の標準クアッドパイプラインにより回転・拡縮・
/// 平行移動・不透明度を適用されるため、ここではローカル座標のみを扱う
/// （world_x/y・opacity等の合成は行わない）。
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
