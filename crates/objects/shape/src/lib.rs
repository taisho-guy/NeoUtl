use neoutl_object_api::{
    Dimensionality, EntryFn, ObjectMeta, ObjectVTable, ParamKind, ParamSchema, RenderContext,
    StrRef, WgslSource,
};
use std::sync::OnceLock;

static WGSL: &str = include_str!("../shape.wgsl");

static PARAM_SCHEMA: &[ParamSchema] = &[
    ParamSchema {
        key: StrRef::from_str("sides"),
        label: StrRef::from_str("辺の数"),
        kind: ParamKind::Float,
        min: 3.0,
        max: 32.0,
        step: 1.0,
        default_float: 4.0,
    },
    ParamSchema {
        key: StrRef::from_str("extrude_depth"),
        label: StrRef::from_str("押し出し量"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 5.0,
        step: 0.01,
        default_float: 0.0,
    },
    ParamSchema {
        key: StrRef::from_str("stroke_width"),
        label: StrRef::from_str("線幅"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 50.0,
        step: 0.5,
        default_float: 0.0,
    },
    ParamSchema {
        key: StrRef::from_str("fill_color"),
        label: StrRef::from_str("塗り色"),
        kind: ParamKind::Color,
        min: 0.0,
        max: 1.0,
        step: 0.0,
        default_float: 1.0,
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
    // MAX_SIDES(32) * 2面(表裏=押し出し用) * 3頂点
    32 * 2 * 3
}
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource {
        ptr: WGSL.as_ptr(),
        len: WGSL.len(),
    }
}
// 実描画はホストの標準Uniform契約(mvp/opacity/sides/extrude_depth/fill_color)を通して行われるため、
// per-object固有処理は不要。将来、頂点バッファ差し替え等が必要になった場合の拡張点として維持する。
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
