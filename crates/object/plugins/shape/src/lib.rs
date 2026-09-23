use neoutl_object_api::{
    Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamKind, ParamSchema, RenderContext,
    StrRef, WgslSource,
};
use std::sync::OnceLock;

static SHAPE_SPV: &[u8] = include_str!(concat!(env!("OUT_DIR"), "/shape.wgsl")).as_bytes();

static PARAM_SCHEMA: &[ParamSchema] = &[
    ParamSchema {
        key: StrRef::from_str("sides"),
        label: StrRef::from_str("辺の数"),
        kind: ParamKind::Float,
        min: 3.0,
        max: 32.0,
        step: 1.0,
        default_float: 4.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("extrude_depth"),
        label: StrRef::from_str("押し出し量"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
        step: 0.01,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("stroke_width"),
        label: StrRef::from_str("線幅"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 50.0,
        step: 0.5,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    ParamSchema {
        key: StrRef::from_str("fill_color"),
        label: StrRef::from_str("塗り色"),
        kind: ParamKind::Color,
        min: 0.0,
        max: 1.0,
        step: 0.0,
        default_float: 1.0,
        enum_options: StrRef::from_str(""),
    },
];

static PROPERTY_GROUPS: &[neoutl_object_api::PropertyGroup] = &[neoutl_object_api::PropertyGroup {
    group_id: neoutl_object_api::StrRef::from_str(neoutl_object_api::DEFAULT_PROPERTY_GROUP_ID),
    schema: neoutl_object_api::FfiSlice::from_static(PARAM_SCHEMA),
}];
static META: ObjectMeta = ObjectMeta {
    stable_id: "neoutl.object.shape",
    name: "Shape",
    dimensionality: Dimensionality::Both,
    property_groups: neoutl_object_api::FfiSlice::from_static(PROPERTY_GROUPS),
};
static VTABLE: OnceLock<ObjectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const ObjectMeta {
    &raw const META
}
unsafe extern "C" fn vertex_count() -> u32 {
    32 * 2 * 3
}
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource {
        ptr: SHAPE_SPV.as_ptr(),
        len: SHAPE_SPV.len(),
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
rust_i18n::i18n!("../../../../i18n");
extern crate rust_i18n;
