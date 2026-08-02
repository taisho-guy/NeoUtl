use neoutl_object_api::{
    Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamSchema, RenderContext, SCENE_STABLE_ID,
    WgslSource,
};
use std::sync::OnceLock;

/// ネイティブUIスキーマはecs::object_schema::SCENE_SCHEMA側で定義する
/// （target_sceneの選択肢がSceneResource.scenesに依存し実行時構築を要するため、
/// 静的FFIスキーマでは表現できない。AUDIO_STABLE_ID等と同様、この配列は空のままとする）。
static PARAM_SCHEMA: &[ParamSchema] = &[];

static META: ObjectMeta = ObjectMeta {
    stable_id: SCENE_STABLE_ID,
    name: "Scene",
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
/// SCENE_STABLE_IDはホスト（renderer::pipeline::render_scene_texture）が
/// 直接描画するため、VIDEO/IMAGE/AUDIO同様にrenderは呼ばれない。
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
