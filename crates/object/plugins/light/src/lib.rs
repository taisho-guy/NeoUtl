use neoutl_object_api::{
    DEFAULT_PROPERTY_GROUP_ID, Dimensionality, EntryFn, FfiSlice, LIGHT_STABLE_ID, ObjectMeta,
    ObjectVTable, ParamKind, ParamSchema, PropertyGroup, RenderContext, StrRef, WgslSource,
};
use std::sync::OnceLock;

static PARAM_SCHEMA: &[ParamSchema] = &[
    ParamSchema {
        key: StrRef::from_str("intensity"),
        label: StrRef::from_str("強度"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 10.0,
        step: 0.05,
        default_float: 1.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("color"),
        label: StrRef::from_str("色"),
        kind: ParamKind::Color,
        min: 0.0,
        max: 1.0,
        step: 0.0,
        default_float: 1.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("radius"),
        label: StrRef::from_str("半径"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 50000.0,
        step: 10.0,
        default_float: 500.0,
        enum_options: StrRef::from_str(""),
    },
];

static PROPERTY_GROUPS: &[PropertyGroup] = &[PropertyGroup {
    group_id: StrRef::from_str(DEFAULT_PROPERTY_GROUP_ID),
    schema: FfiSlice::from_static(PARAM_SCHEMA),
}];

static META: ObjectMeta = ObjectMeta {
    stable_id: LIGHT_STABLE_ID,
    name: "Light",
    dimensionality: Dimensionality::ThreeD,
    property_groups: FfiSlice::from_static(PROPERTY_GROUPS),
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
