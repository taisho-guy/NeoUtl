use crate::ecs::effects::ParamKind;

#[derive(Clone, Copy, Debug)]
pub enum Range {
    Fixed(f32, f32),
    StageWidth,
    StageHeight,
    StageDiag,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamSchema {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub range: Range,
    pub enum_options: &'static [&'static str],
    pub depends_on: Option<&'static str>,
    pub depends_eq: f32,
}

pub const fn dep(mut schema: ParamSchema, on: &'static str, eq: f32) -> ParamSchema {
    schema.depends_on = Some(on);
    schema.depends_eq = eq;
    schema
}

pub fn is_visible(schema: &ParamSchema, get: impl Fn(&str) -> f32) -> bool {
    match schema.depends_on {
        None => true,
        Some(key) => (get(key) - schema.depends_eq).abs() < f32::EPSILON,
    }
}

const fn float_fixed(key: &'static str, label: &'static str, min: f32, max: f32) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Float,
        range: Range::Fixed(min, max),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn float_stage(key: &'static str, label: &'static str, range: Range) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Float,
        range,
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn bool_field(key: &'static str, label: &'static str) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Bool,
        range: Range::Fixed(0.0, 1.0),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn text_field(key: &'static str, label: &'static str) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Text,
        range: Range::Fixed(0.0, 0.0),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn enum_field(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
) -> ParamSchema {
    ParamSchema {
        key,
        label,
        kind: ParamKind::Enum,
        range: Range::Fixed(0.0, (options.len() - 1) as f32),
        enum_options: options,
        depends_on: None,
        depends_eq: 0.0,
    }
}

pub const TRANSFORM_SCHEMA: &[ParamSchema] = &[
    float_stage("x", "X", Range::StageWidth),
    float_stage("y", "Y", Range::StageHeight),
    float_stage("z", "Z", Range::StageDiag),
    float_fixed("scale_x", "拡大率X", 0.0, 10.0),
    float_fixed("scale_y", "拡大率Y", 0.0, 10.0),
    float_fixed("rot_x", "X軸回転", -360.0, 360.0),
    float_fixed("rot_y", "Y軸回転", -360.0, 360.0),
    float_fixed("rot_z", "Z軸回転", -360.0, 360.0),
    float_fixed("opacity", "不透明度", 0.0, 1.0),
];

pub const TEXT_SCHEMA: &[ParamSchema] = &[
    text_field("text", "本文"),
    float_fixed("font_size", "フォントサイズ", 1.0, 500.0),
];

pub const SHAPE_SCHEMA: &[ParamSchema] = &[
    float_fixed("sides", "辺の数", 3.0, 32.0),
    float_fixed("extrude_depth", "押し出し量", 0.0, 5.0),
    float_fixed("stroke_width", "線幅", 0.0, 50.0),
    float_fixed("fill_r", "塗りR", 0.0, 1.0),
    float_fixed("fill_g", "塗りG", 0.0, 1.0),
    float_fixed("fill_b", "塗りB", 0.0, 1.0),
    float_fixed("fill_a", "塗りA", 0.0, 1.0),
];

pub const AUDIO_SCHEMA: &[ParamSchema] = &[
    float_fixed("volume", "音量", 0.0, 2.0),
    dep(float_fixed("pan", "パン", -1.0, 1.0), "mute", 0.0),
    bool_field("mute", "ミュート"),
];

pub const GROUP_CONTROL_SCHEMA: &[ParamSchema] = &[
    float_fixed("layer_count_down", "対象レイヤー数(下)", 0.0, 100.0),
    float_fixed("layer_count_up", "対象レイヤー数(上)", 0.0, 100.0),
    bool_field("generate_framebuffer", "フレームバッファを生成"),
    dep(
        bool_field("hide_captured", "補足オブジェクトを描画しない"),
        "generate_framebuffer",
        1.0,
    ),
];

const CLIP_MODE_OPTIONS: &[&str] = &["アルファ", "アルファ反転", "輝度", "輝度反転", "クロマキー"];

pub const CLIP_TARGET_SCHEMA: &[ParamSchema] = &[
    bool_field("enabled", "クリッピング対象を設定する"),
    float_fixed("layer_count_down", "対象レイヤー数(下)", 0.0, 100.0),
    float_fixed("layer_count_up", "対象レイヤー数(上)", 0.0, 100.0),
    enum_field("mode", "モード", CLIP_MODE_OPTIONS),
    dep(
        float_fixed("chroma_hue", "色相(度)", 0.0, 360.0),
        "mode",
        4.0,
    ),
    dep(
        float_fixed("chroma_tolerance", "許容角度(度)", 0.0, 180.0),
        "mode",
        4.0,
    ),
    bool_field("blend_edge", "境界を滑らかにする"),
    bool_field("render_self", "オブジェクト自身を描画する"),
];

pub const CLIP_TARGET_ENABLED_KEY: &str = "enabled";

pub fn resolve_range(range: Range, stage_width: f32, stage_height: f32) -> (f32, f32) {
    match range {
        Range::Fixed(min, max) => (min, max),
        Range::StageWidth => (-stage_width / 2.0, stage_width / 2.0),
        Range::StageHeight => (-stage_height / 2.0, stage_height / 2.0),
        Range::StageDiag => (-stage_width, stage_width),
    }
}
