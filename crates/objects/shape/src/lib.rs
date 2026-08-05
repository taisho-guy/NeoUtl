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

static META: ObjectMeta = ObjectMeta {
    stable_id: "neoutl.object.shape",
    name: "Shape",
    dimensionality: Dimensionality::Both,
    property_schema_ptr: PARAM_SCHEMA.as_ptr(),
    property_schema_len: PARAM_SCHEMA.len(),
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
rust_i18n::i18n!("../../../i18n");
#[macro_use]
extern crate rust_i18n;
