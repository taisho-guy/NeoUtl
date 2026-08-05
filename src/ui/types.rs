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

/// category: エフェクトプラグインのEffectMeta.categoryをそのまま転記する。
/// ホスト側でカテゴリ名を固定管理しないため、新規プラグイン追加時もコード変更不要。
#[derive(Clone, Debug, Default)]
pub struct CatalogRow {
    pub id: String,
    pub name: String,
    pub category: String,
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
    /// アイコン名（Lucide命名規則）。egui側にアイコンフォント/テクスチャ描画が
    /// 未実装のため現状は読み出されない（実装時にtimeline.rsのButton描画へ渡す）。
    #[allow(dead_code)]
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct SceneTabItem {
    pub id: i32,
    pub name: String,
    pub active: bool,
}
