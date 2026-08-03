use egui::TextureId;

#[derive(Clone, Debug, Default)]
pub struct TimelineObject {
    pub id: i32,
    pub start_frame: i32,
    pub end_frame: i32,
    pub kind: i32,
    pub kind_known: bool,
    pub layer: i32,
    pub label: String,
    pub selected: bool,
    pub keyframe_frames: Vec<i32>,
    pub waveform: Option<TextureId>,
    pub has_waveform: bool,
    pub waveform_origin_frame: i32,
    pub waveform_duration_frames: i32,
}

/// Trackパラメータ（他オブジェクト参照）の選択候補1件。idはobject_id。
#[derive(Clone, Debug, Default)]
pub struct TrackOption {
    pub id: i32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct ObjectKindItem {
    pub kind: i32,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayerState {
    pub visible: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, Default)]
pub struct EffectRow {
    pub index: i32,
    pub name: String,
    pub enabled: bool,
    pub dragging: bool,
}

/// kind: 0=Float, 1=Bool, 2=Color, 3=Enum, 4=Text, 5=FilePath, 6=Track
/// （neoutl-shared-abi::ParamKindの数値と一致させること）
/// effect_index: -1=オブジェクト直下パラメータ, 0以上=エフェクトスタックのインデックス
/// group: セクション見出し（トランスフォーム/テキスト/図形/オーディオ/プラグイン名、エフェクトはエフェクト名）
/// text: kind==4(Text)またはkind==5(FilePath)のときのみ使用する文字列値。それ以外は空文字。
/// enum_options: kind==3(Enum)のときのみ使用する選択肢表示名列。
/// enum_index: kind==3(Enum)のとき選択中の選択肢index。kind==6(Track)のときtrack_options内の選択index（未選択は-1）。
#[derive(Clone, Debug, Default)]
pub struct ParamRow {
    pub effect_index: i32,
    pub key: String,
    pub label: String,
    pub group: String,
    pub value: f32,
    pub kind: i32,
    pub min: f32,
    pub max: f32,
    pub text: String,
    pub enum_options: Vec<String>,
    pub enum_index: i32,
    pub track_options: Vec<TrackOption>,
    /// track_options[i].nameを事前展開した表示名列。egui ComboBoxが直接参照する
    /// 平坦文字列配列として、track_optionsと並行して保持する。
    pub track_names: Vec<String>,
    pub has_keyframes: bool,
    /// 中間点トラック（AviQtl準拠）の描画用。フレーム番号の昇順配列（境界を含まない内部点のみ）。
    pub keyframe_frames: Vec<i32>,
    /// 境界フレーム列（昇順・重複無し）。先頭=clip_start、末尾=clip_end、
    /// 中間=既存中間点。要素数は常に2以上。区間トラック描画・区間ハイライトの
    /// 両方が参照する唯一の点集合。
    pub boundary_frames: Vec<i32>,
    /// 現在フレームを内包する区間の両端（絶対フレーム）。
    pub segment_start_frame: i32,
    pub segment_end_frame: i32,
    /// 上記区間の両端実効値（neoutl_interp::evaluate結果）。
    pub segment_start_value: f32,
    pub segment_end_value: f32,
}

/// category: エフェクトプラグインのEffectMeta.categoryをそのまま転記する。
/// ホスト側でカテゴリ名を固定管理しないため、新規プラグイン追加時もコード変更不要。
#[derive(Clone, Debug, Default)]
pub struct CatalogRow {
    pub id: String,
    pub name: String,
    pub category: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectListItem {
    pub name: String,
    pub path: String,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectTabItem {
    pub index: i32,
    pub name: String,
    pub active: bool,
}

/// タイムライン右クリックメニューの1項目。項目集合はRust側
/// (timeline.rs::build_context_menu)が唯一の生成元であり、egui側は
/// action値による分岐でクリック処理を振り分けるのみとする（描画専任）。
/// action: 0=Split at Playhead, 1=Delete Object, 2=Add Object(kind使用),
///         3=Toggle Ripple Mode, 4=区切り線（クリック不可・表示のみ）,
///         5=Undo, 6=Redo, 7=Duplicate, 8=Cut, 9=Copy, 10=Paste
#[derive(Clone, Debug, Default)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: i32,
    pub kind: i32,
    pub enabled: bool,
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct SceneTabItem {
    pub id: i32,
    pub name: String,
    pub active: bool,
}
