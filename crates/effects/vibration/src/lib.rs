use neoutl_effect_api::{
    EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource, pack_uniform_std,
    uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SPV: &[u8] = include_str!(concat!(env!("OUT_DIR"), "/vibration.wgsl")).as_bytes();

static PARAM_SCHEMA: &[EffectParamSchema] = &[
    EffectParamSchema {
        key: StrRef::from_str("amplitude"),
        label: StrRef::from_str("振幅"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 100.0,
        step: 1.0,
        default_float: 4.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("frequency"),
        label: StrRef::from_str("周波数"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 60.0,
        step: 0.6,
        default_float: 10.0,
        enum_options: StrRef::from_str(""),
    },
];

static META: EffectMeta = EffectMeta {
    id: "vibration",
    name: "Vibration",
    category: "Distort",
    param_schema_ptr: PARAM_SCHEMA.as_ptr(),
    param_schema_len: PARAM_SCHEMA.len(),
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
    })
}

const _: neoutl_effect_api::EntryFn = neoutl_effect_entry;
rust_i18n::i18n!("../../../i18n");
#[macro_use]
extern crate rust_i18n;
