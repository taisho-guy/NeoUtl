use neoutl_effect_api::{
    EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource, pack_uniform_std,
    uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SPV: &[u8] = include_str!(concat!(env!("OUT_DIR"), "/mosaic.wgsl")).as_bytes();

static PARAM_SCHEMA: &[EffectParamSchema] = &[EffectParamSchema {
    key: StrRef::from_str("cell_size"),
    label: StrRef::from_str("セルサイズ"),
    kind: ParamKind::Float,
    min: 1.0,
    max: 200.0,
    step: 1.99,
    default_float: 16.0,
    enum_options: StrRef::from_str(""),
}];

static META: EffectMeta = EffectMeta {
    id: "mosaic",
    name: "Mosaic",
    category: "Blur",
    param_schema: neoutl_effect_api::FfiSlice::from_static(PARAM_SCHEMA),
};
static VTABLE: OnceLock<EffectVTable> = OnceLock::new();

unsafe extern "C" fn meta() -> *const EffectMeta {
    &raw const META
}
unsafe extern "C" fn wgsl() -> WgslSource {
    WgslSource {
        ptr: FRAGMENT_SPV.as_ptr(),
        len: FRAGMENT_SPV.len(),
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
        requires_texture_param: None,
    })
}

const _: neoutl_effect_api::EntryFn = neoutl_effect_entry;
rust_i18n::i18n!("../../../i18n");
extern crate rust_i18n;
