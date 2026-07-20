#![allow(non_camel_case_types)]

pub use neoutl_shared_abi::{Dimensionality, ParamKind, ParamSchema, StrRef, WgslSource};

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

/// 動画オブジェクトの予約stable_id。
/// デコードはホスト専有のMediaCacheが担うため、TEXT_STABLE_ID同様にrenderを呼ばず
/// ホストがMediaSourceコンポーネントを直接読み取ってテクスチャ描画する。
pub const VIDEO_STABLE_ID: &str = "neoutl.object.video";

/// 画像オブジェクトの予約stable_id。動画と同一のホスト直接描画契約に従う。
pub const IMAGE_STABLE_ID: &str = "neoutl.object.image";

/// 音声オブジェクトの予約stable_id。視覚描画を持たずAudioParams経由の
/// 音量・パン制御のみをタイムライン上で保持する。
pub const AUDIO_STABLE_ID: &str = "neoutl.object.audio";
