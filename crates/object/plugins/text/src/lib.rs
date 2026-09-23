use neoutl_object_api::{
    Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamKind, ParamSchema, RenderContext,
    StrRef, TEXT_STABLE_ID, WgslSource,
};
use std::sync::OnceLock;

static PARAM_SCHEMA: &[ParamSchema] = &[
    ParamSchema {
        key: StrRef::from_str("font_size"),
        label: StrRef::from_str("フォントサイズ"),
        kind: ParamKind::Float,
        min: 1.0,
        max: 500.0,
        step: 1.0,
        default_float: 48.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("bold"),
        label: StrRef::from_str("太字"),
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("italic"),
        label: StrRef::from_str("斜体"),
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("line_height"),
        label: StrRef::from_str("行間"),
        kind: ParamKind::Float,
        min: 0.5,
        max: 3.0,
        step: 0.1,
        default_float: 1.2,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("outline_width"),
        label: StrRef::from_str("縁取り幅"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 50.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
];

static PROPERTY_GROUPS: &[neoutl_object_api::PropertyGroup] = &[neoutl_object_api::PropertyGroup {
    group_id: neoutl_object_api::StrRef::from_str(neoutl_object_api::DEFAULT_PROPERTY_GROUP_ID),
    schema: neoutl_object_api::FfiSlice::from_static(PARAM_SCHEMA),
}];
static META: ObjectMeta = ObjectMeta {
    stable_id: TEXT_STABLE_ID,
    name: "Text",
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
