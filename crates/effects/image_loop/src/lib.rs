use neoutl_effect_api::{
    EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource, pack_uniform_std,
    uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SRC: &str = include_str!("../image_loop.wgsl");
static FRAGMENT_WGSL: OnceLock<String> = OnceLock::new();

static PARAM_SCHEMA: &[EffectParamSchema] = &[EffectParamSchema {
    key: StrRef::from_str("enabled"),
    label: StrRef::from_str("有効"),
    kind: ParamKind::Bool,
    min: 0.0,
    max: 1.0,
    step: 1.0,
    default_float: 1.0,
}];

static META: EffectMeta = EffectMeta {
    id: "image_loop",
    name: "ImageLoop",
    category: "Utility",
    param_schema_ptr: PARAM_SCHEMA.as_ptr(),
    param_schema_len: PARAM_SCHEMA.len(),
};
static VTABLE: OnceLock<EffectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const EffectMeta {
    &raw const META
}
unsafe extern "C" fn wgsl() -> WgslSource {
    let s = FRAGMENT_WGSL
        .get_or_init(|| format!("{}{}", neoutl_effect_api::VERTEX_PRELUDE_WGSL, FRAGMENT_SRC));
    WgslSource {
        ptr: s.as_ptr(),
        len: s.len(),
    }
}
unsafe extern "C" fn uniform_size() -> u32 {
    uniform_size_std(PARAM_SCHEMA.len() as u32)
}
unsafe extern "C" fn pack_uniform(params_ptr: *const f32, count: u32, out_ptr: *mut u8) {
    unsafe { pack_uniform_std(params_ptr, count, out_ptr) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_effect_entry() -> *const EffectVTable {
    VTABLE.get_or_init(|| EffectVTable {
        meta,
        wgsl,
        uniform_size,
        pack_uniform,
    })
}

const _: neoutl_effect_api::EntryFn = neoutl_effect_entry;
