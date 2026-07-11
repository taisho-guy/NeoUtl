#![allow(non_camel_case_types)]

/// オブジェクトの対応次元。ホストはこの値でカメラ行列（Ortho / Perspective）を切替える。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimensionality {
    TwoD = 0,
    ThreeD = 1,
    Both = 2,
}

/// 設定ダイアログUI生成用のパラメータ種別。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Float = 0,
    Bool = 1,
    Color = 2,
    Enum = 3,
}

/// C ABI越しに渡す固定長文字列参照。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StrRef {
    pub ptr: *const u8,
    pub len: usize,
}

impl StrRef {
    pub const fn from_str(s: &'static str) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }
}
unsafe impl Send for StrRef {}
unsafe impl Sync for StrRef {}

/// float既定値のみ格納。Bool/Enumはdefault_floatを0/1として解釈する。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParamSchema {
    pub key: StrRef,
    pub label: StrRef,
    pub kind: ParamKind,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default_float: f32,
}

#[repr(C)]
pub struct ObjectMeta {
    pub stable_id: &'static str,
    pub name: &'static str,
    pub dimensionality: Dimensionality,
    pub property_schema_ptr: *const ParamSchema,
    pub property_schema_len: usize,
}
unsafe impl Send for ObjectMeta {}
unsafe impl Sync for ObjectMeta {}

/// ホスト側が算出した合成行列（Model * View * Projection、列優先）を都度渡す。
/// プラグインはこれをuniformへそのまま書き込むだけでよい。
#[repr(C)]
pub struct RenderContext {
    pub version: u32,
    pub render_pass_ptr: *mut (),
    pub bind_group_ptr: *const (),
    pub vertex_count: u32,
    pub mvp_matrix: [f32; 16],
    pub opacity: f32,
    pub depth_enabled: bool,
}

#[repr(C)]
pub struct WgslSource {
    pub ptr: *const u8,
    pub len: usize,
}
unsafe impl Send for WgslSource {}
unsafe impl Sync for WgslSource {}

#[repr(C)]
pub struct ObjectVTable {
    pub meta: unsafe extern "C" fn() -> *const ObjectMeta,
    pub vertex_count: unsafe extern "C" fn() -> u32,
    pub wgsl: unsafe extern "C" fn() -> WgslSource,
    pub render: unsafe extern "C" fn(ctx: *const RenderContext),
}

/// 全ObjectVTable実装が頂点を記述する際のローカル座標契約。
/// 原点中心・辺/半径0.5相当（=直径1.0）の正規化スケールで記述すること
/// （例: shape.wgslの単位円）。ホストはこの直径をUNIT_SIZE_PXピクセルとして
/// world空間（ピクセル座標系）へ解釈する。Transform.scale_x/y=1.0のとき
/// オブジェクトの実寸はUNIT_SIZE_PXピクセルになる。
pub const UNIT_SIZE_PX: f32 = 200.0;

pub const ENTRY_SYMBOL: &[u8] = b"neoutl_object_entry\0";
pub type EntryFn = unsafe extern "C" fn() -> *const ObjectVTable;

/// テキストオブジェクトの予約stable_id。
/// フォント・IME・レイアウトはホスト専有APIに依存するため、
/// このIDのオブジェクトはObjectVTable.renderを呼ばずホストが直接描画する。
pub const TEXT_STABLE_ID: &str = "neoutl.object.text";
