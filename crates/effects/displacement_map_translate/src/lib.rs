use neoutl_effect_api::{
    EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource, pack_uniform_std,
    uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SPV: &[u8] =
    include_str!(concat!(env!("OUT_DIR"), "/displacement_map_translate.wgsl")).as_bytes();

static PARAM_SCHEMA: &[EffectParamSchema] = &[
    EffectParamSchema {
        key: StrRef::from_str("map_shape"),
        label: StrRef::from_str("マップの種類"),
        kind: ParamKind::Enum,
        min: 0.0,
        max: 6.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str("円\0四角形\0三角形\0五角形\0六角形\0星型\0背景"),
    },
    EffectParamSchema {
        key: StrRef::from_str("pos_x"),
        label: StrRef::from_str("X"),
        kind: ParamKind::Float,
        min: -4000.0,
        max: 4000.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("pos_y"),
        label: StrRef::from_str("Y"),
        kind: ParamKind::Float,
        min: -4000.0,
        max: 4000.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("rotation"),
        label: StrRef::from_str("回転"),
        kind: ParamKind::Float,
        min: -3600.0,
        max: 3600.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("size"),
        label: StrRef::from_str("サイズ"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 4000.0,
        step: 1.0,
        default_float: 200.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("aspect"),
        label: StrRef::from_str("縦横比"),
        kind: ParamKind::Float,
        min: -100.0,
        max: 100.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("feather"),
        label: StrRef::from_str("ぼかし"),
        kind: ParamKind::Float,
        min: 0.0,
        max: 1000.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("disp_x"),
        label: StrRef::from_str("変形X"),
        kind: ParamKind::Float,
        min: -4000.0,
        max: 4000.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
    EffectParamSchema {
        key: StrRef::from_str("disp_y"),
        label: StrRef::from_str("変形Y"),
        kind: ParamKind::Float,
        min: -4000.0,
        max: 4000.0,
        step: 1.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
    },
];

static META: EffectMeta = EffectMeta {
    id: "displacement_map_translate",
    name: "DisplacementMap(Translate)",
    category: "Distortion",
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
        requires_texture_param: None,
    })
}

const _: neoutl_effect_api::EntryFn = neoutl_effect_entry;
rust_i18n::i18n!("../../../i18n");
extern crate rust_i18n;

#[cfg(test)]
mod tests {
    fn variance(luminance: f32) -> f32 {
        (luminance - 0.5) * 2.0
    }

    #[test]
    fn variance_black() {
        assert_eq!(variance(0.0), -1.0);
    }

    #[test]
    fn variance_mid() {
        assert_eq!(variance(0.5), 0.0);
    }

    #[test]
    fn variance_white() {
        assert_eq!(variance(1.0), 1.0);
    }
}
