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
    /// プロパティパネルの折り畳みセクション名。UI側の見出しグルーピングは未実装のため
    /// 現状は読み出されない（実装時にproperties.rsのグループ描画へ渡す）。
    #[allow(dead_code)]
    pub group: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub range: Range,
    /// kind==Enumのときのみ選択肢を保持する。それ以外のkindでは空スライス。
    /// EnumのComboBox描画が未実装のため現状は読み出されない。
    #[allow(dead_code)]
    pub enum_options: &'static [&'static str],
    /// 表示条件（設定項目の動的な増減）。同一グループ内の他キーの現在値がdepends_eqと
    /// 一致する場合のみUI上に表示する。Noneの場合は常時表示。
    /// 対象キーがBoolの場合は1.0=true/0.0=falseとの比較、Enumの場合はインデックス比較になる。
    pub depends_on: Option<&'static str>,
    pub depends_eq: f32,
}

/// depends_on/depends_eqを付与する。const文脈で既存ビルダーの結果を包む形で使う。
/// 例: dep(bool_field(GROUP, "pan", "パン"), "mute", 0.0)
pub const fn dep(mut schema: ParamSchema, on: &'static str, eq: f32) -> ParamSchema {
    schema.depends_on = Some(on);
    schema.depends_eq = eq;
    schema
}

/// depends_onが未設定なら常にtrue。
pub fn is_visible(schema: &ParamSchema, get: impl Fn(&str) -> f32) -> bool {
    match schema.depends_on {
        None => true,
        Some(key) => (get(key) - schema.depends_eq).abs() < f32::EPSILON,
    }
}

const fn float_fixed(
    group: &'static str,
    key: &'static str,
    label: &'static str,
    min: f32,
    max: f32,
) -> ParamSchema {
    ParamSchema {
        group,
        key,
        label,
        kind: ParamKind::Float,
        range: Range::Fixed(min, max),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn float_stage(
    group: &'static str,
    key: &'static str,
    label: &'static str,
    range: Range,
) -> ParamSchema {
    ParamSchema {
        group,
        key,
        label,
        kind: ParamKind::Float,
        range,
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

const fn bool_field(group: &'static str, key: &'static str, label: &'static str) -> ParamSchema {
    ParamSchema {
        group,
        key,
        label,
        kind: ParamKind::Bool,
        range: Range::Fixed(0.0, 1.0),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

/// 文字列専用フィールド。数値min/max/stepは不使用（Range::Fixed(0.0, 0.0)はダミー値）。
const fn text_field(group: &'static str, key: &'static str, label: &'static str) -> ParamSchema {
    ParamSchema {
        group,
        key,
        label,
        kind: ParamKind::Text,
        range: Range::Fixed(0.0, 0.0),
        enum_options: &[],
        depends_on: None,
        depends_eq: 0.0,
    }
}

pub const TRANSFORM_GROUP: &str = "トランスフォーム";
pub const TEXT_GROUP: &str = "テキスト";
pub const SHAPE_GROUP: &str = "図形";
pub const AUDIO_GROUP: &str = "オーディオ";

pub const TRANSFORM_SCHEMA: &[ParamSchema] = &[
    float_stage(TRANSFORM_GROUP, "x", "X", Range::StageWidth),
    float_stage(TRANSFORM_GROUP, "y", "Y", Range::StageHeight),
    float_stage(TRANSFORM_GROUP, "z", "Z", Range::StageDiag),
    float_fixed(TRANSFORM_GROUP, "scale_x", "拡大率X", 0.0, 10.0),
    float_fixed(TRANSFORM_GROUP, "scale_y", "拡大率Y", 0.0, 10.0),
    float_fixed(TRANSFORM_GROUP, "rot_x", "X軸回転", -360.0, 360.0),
    float_fixed(TRANSFORM_GROUP, "rot_y", "Y軸回転", -360.0, 360.0),
    float_fixed(TRANSFORM_GROUP, "rot_z", "Z軸回転", -360.0, 360.0),
    float_fixed(TRANSFORM_GROUP, "opacity", "不透明度", 0.0, 1.0),
];

pub const TEXT_SCHEMA: &[ParamSchema] = &[
    text_field(TEXT_GROUP, "text", "本文"),
    float_fixed(TEXT_GROUP, "font_size", "フォントサイズ", 1.0, 500.0),
];

pub const SHAPE_SCHEMA: &[ParamSchema] = &[
    float_fixed(SHAPE_GROUP, "sides", "辺の数", 3.0, 32.0),
    float_fixed(SHAPE_GROUP, "extrude_depth", "押し出し量", 0.0, 5.0),
    float_fixed(SHAPE_GROUP, "stroke_width", "線幅", 0.0, 50.0),
    float_fixed(SHAPE_GROUP, "fill_r", "塗りR", 0.0, 1.0),
    float_fixed(SHAPE_GROUP, "fill_g", "塗りG", 0.0, 1.0),
    float_fixed(SHAPE_GROUP, "fill_b", "塗りB", 0.0, 1.0),
    float_fixed(SHAPE_GROUP, "fill_a", "塗りA", 0.0, 1.0),
];

pub const AUDIO_SCHEMA: &[ParamSchema] = &[
    float_fixed(AUDIO_GROUP, "volume", "音量", 0.0, 2.0),
    dep(
        float_fixed(AUDIO_GROUP, "pan", "パン", -1.0, 1.0),
        "mute",
        0.0,
    ),
    bool_field(AUDIO_GROUP, "mute", "ミュート"),
];

pub fn resolve_range(range: Range, stage_width: f32, stage_height: f32) -> (f32, f32) {
    match range {
        Range::Fixed(min, max) => (min, max),
        Range::StageWidth => (-stage_width / 2.0, stage_width / 2.0),
        Range::StageHeight => (-stage_height / 2.0, stage_height / 2.0),
        Range::StageDiag => (-stage_width, stage_width),
    }
}
