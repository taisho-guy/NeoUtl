use neoutl_object_api::{
    AUDIO_STABLE_ID, Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamSchema, RenderContext,
    WgslSource,
};
use std::sync::OnceLock;

// 音声は視覚描画を持たないため、vertex_count=0・wgsl無し・render空実装に固定する。
// AUDIO_STABLE_IDの検出時、ホストはpipelines/media_pipelineいずれの描画経路にも進まず、
// AudioParams（volume/pan/mute）のみタイムライン上のクリップとして保持・編集する。
static PARAM_SCHEMA: &[ParamSchema] = &[];

static META: ObjectMeta = ObjectMeta {
    stable_id: AUDIO_STABLE_ID,
    name: "Audio",
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
