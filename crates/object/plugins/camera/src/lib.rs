use neoutl_object_api::{
    CAMERA_STABLE_ID, DEFAULT_PROPERTY_GROUP_ID, Dimensionality, EntryFn, FfiSlice, ObjectMeta,
    ObjectVTable, ParamKind, ParamSchema, PropertyGroup, RenderContext, StrRef, WgslSource,
};
use std::sync::OnceLock;

static PARAM_SCHEMA: &[ParamSchema] = &[
    ParamSchema {
        key: StrRef::from_str("fov_deg"),
        label: StrRef::from_str("画角"),
        kind: ParamKind::Float,
        min: 1.0,
        max: 179.0,
        step: 0.5,
        default_float: 45.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("near"),
        label: StrRef::from_str("近クリップ"),
        kind: ParamKind::Float,
        min: 0.1,
        max: 100000.0,
        step: 1.0,
        default_float: 1.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("far"),
        label: StrRef::from_str("遠クリップ"),
        kind: ParamKind::Float,
        min: 1.0,
        max: 1000000.0,
        step: 10.0,
        default_float: 50000.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("tilt_deg"),
        label: StrRef::from_str("傾き"),
        kind: ParamKind::Float,
        min: -180.0,
        max: 180.0,
        step: 0.5,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("zbuffer_enabled"),
        label: StrRef::from_str("Zバッファ有効"),
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
];

static PROPERTY_GROUPS: &[PropertyGroup] = &[PropertyGroup {
    group_id: StrRef::from_str(DEFAULT_PROPERTY_GROUP_ID),
    schema: FfiSlice::from_static(PARAM_SCHEMA),
}];

static META: ObjectMeta = ObjectMeta {
    stable_id: CAMERA_STABLE_ID,
    name: "Camera",
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
