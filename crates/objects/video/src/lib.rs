use neoutl_object_api::{
    Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamSchema, RenderContext, VIDEO_STABLE_ID,
    WgslSource,
};
use std::sync::OnceLock;

// デコード・フレームテクスチャ取得はホスト専有のMediaCacheが担うため、
// GPU頂点/WGSLを持たずvertex_count=0を返す。ホストはVIDEO_STABLE_IDを検出した場合、
// このVTable.renderを呼ばず、MediaSourceコンポーネントを直接media_pipelineへ描画する。
static PARAM_SCHEMA: &[ParamSchema] = &[];

static META: ObjectMeta = ObjectMeta {
    stable_id: VIDEO_STABLE_ID,
    name: "Video",
    dimensionality: Dimensionality::TwoD,
    property_schema_ptr: PARAM_SCHEMA.as_ptr(),
    property_schema_len: PARAM_SCHEMA.len(),
};
static VTABLE: OnceLock<ObjectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const ObjectMeta {
    &raw const META
}
unsafe extern "C" fn vertex_count() -> u32 {
    0
}
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource {
        ptr: std::ptr::null(),
        len: 0,
    }
}
unsafe extern "C" fn render(_ctx: *const RenderContext) {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_object_entry() -> *const ObjectVTable {
    VTABLE.get_or_init(|| ObjectVTable {
        meta,
        vertex_count,
        wgsl,
        render,
    })
}

const _: EntryFn = neoutl_object_entry;
