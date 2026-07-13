use neoutl_effect_api::{
    EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource, pack_uniform_std,
    uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SRC: &str = include_str!("../color_correction.wgsl");
static FRAGMENT_WGSL: OnceLock<String> = OnceLock::new();

static PARAM_SCHEMA: &[EffectParamSchema] = &[
    EffectParamSchema {
        key: StrRef::from_str("brightness"),
        label: StrRef::from_str("明度"),
        kind: ParamKind::Float,
        min: -1.0,
        max: 1.0,
        step: 0.02,
        default_float: 0.0,
    },
    EffectParamSchema {
        key: StrRef::from_str("contrast"),
        label: StrRef::from_str("コントラスト"),
        kind: ParamKind::Float,
        min: -1.0,
        max: 1.0,
        step: 0.02,
        default_float: 0.0,
    },
    EffectParamSchema {
        key: StrRef::from_str("saturation"),
        label: StrRef::from_str("彩度"),
        kind: ParamKind::Float,
        min: -1.0,
        max: 1.0,
        step: 0.02,
        default_float: 0.0,
    },
    EffectParamSchema {
        key: StrRef::from_str("hue"),
        label: StrRef::from_str("色相"),
        kind: ParamKind::Float,
        min: -180.0,
        max: 180.0,
        step: 3.6,
        default_float: 0.0,
    },
];

static META: EffectMeta = EffectMeta {
    id: "color_correction",
    name: "ColorCorrection",
    category: "Color",
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
