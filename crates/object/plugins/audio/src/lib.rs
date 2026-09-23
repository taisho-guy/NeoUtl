use neoutl_object_api::{
    AUDIO_STABLE_ID, Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamSchema, RenderContext,
    WgslSource,
};
use std::sync::OnceLock;

static PARAM_SCHEMA: &[ParamSchema] = &[];

static PROPERTY_GROUPS: &[neoutl_object_api::PropertyGroup] = &[neoutl_object_api::PropertyGroup {
    group_id: neoutl_object_api::StrRef::from_str(neoutl_object_api::DEFAULT_PROPERTY_GROUP_ID),
    schema: neoutl_object_api::FfiSlice::from_static(PARAM_SCHEMA),
}];
static META: ObjectMeta = ObjectMeta {
    stable_id: AUDIO_STABLE_ID,
    name: "Audio",
    dimensionality: Dimensionality::TwoD,
    property_groups: neoutl_object_api::FfiSlice::from_static(PROPERTY_GROUPS),
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
unsafe extern "C" fn setup_accelerator(
    accelerator: *const neoutl_object_api::AcceleratorHandle,
) -> u32 {
    if accelerator.is_null() {
        return 1;
    }
    let handle = unsafe { &*accelerator };
    if handle.version != neoutl_object_api::AcceleratorHandle::CURRENT_VERSION {
        return 2;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_object_entry() -> *const ObjectVTable {
    VTABLE.get_or_init(|| ObjectVTable {
        meta,
        vertex_count,
        wgsl,
        render,
        read_ref_layer: None,
        setup_accelerator: Some(setup_accelerator),
    })
}

const _: EntryFn = neoutl_object_entry;
