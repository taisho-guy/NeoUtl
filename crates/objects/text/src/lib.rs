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
    },
    ParamSchema {
        key: StrRef::from_str("bold"),
        label: StrRef::from_str("太字"),
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default_float: 0.0,
    },
    ParamSchema {
        key: StrRef::from_str("italic"),
        label: StrRef::from_str("斜体"),
        kind: ParamKind::Bool,
        min: 0.0,
        max: 1.0,
        step: 1.0,
        default_float: 0.0,
    },
];

static META: ObjectMeta = ObjectMeta {
    stable_id: TEXT_STABLE_ID,
    name: "Text",
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
