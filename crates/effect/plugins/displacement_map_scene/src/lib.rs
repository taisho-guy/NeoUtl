use neoutl_effect_api::{
    EffectKind, EffectMeta, EffectParamSchema, EffectVTable, ParamKind, StrRef, WgslSource,
    pack_uniform_std, uniform_size_std,
};
use std::sync::OnceLock;

static FRAGMENT_SPV: &[u8] =
    include_str!(concat!(env!("OUT_DIR"), "/displacement_map_scene.wgsl")).as_bytes();

const MAP_SCENE_SCHEMA_INDEX: u32 = 0;

static PARAM_SCHEMA: &[EffectParamSchema] = &[
    EffectParamSchema {
        key: StrRef::from_str("map_scene"),
        label: StrRef::from_str("参照シーン"),
        kind: ParamKind::Track,
        min: 0.0,
        max: 0.0,
        step: 0.0,
        default_float: 0.0,
        enum_options: StrRef::from_str(""),
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
    id: "displacement_map_scene",
    name: "DisplacementMap(Scene)",
    category: "Distortion",
    param_schema: neoutl_effect_api::FfiSlice::from_static(PARAM_SCHEMA),
    kind: EffectKind::Image,
    author: StrRef::from_str("NeoUtl"),
    description: StrRef::empty(),
    uuid: StrRef::from_str("displacement_map_scene"),
    is_dummy: 0,
    use_composition_camera: 0,
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
unsafe extern "C" fn requires_texture_param() -> u32 {
    MAP_SCENE_SCHEMA_INDEX
}

unsafe extern "C" fn setup_accelerator(
    accelerator: *const neoutl_effect_api::AcceleratorHandle,
) -> u32 {
    if accelerator.is_null() {
        return 1;
    }
    let handle = unsafe { &*accelerator };
    if handle.version != neoutl_effect_api::AcceleratorHandle::CURRENT_VERSION {
        return 2;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn neoutl_effect_entry() -> *const EffectVTable {
    VTABLE.get_or_init(|| EffectVTable {
        meta,
        wgsl,
        uniform_size,
        pack_uniform,
        requires_texture_param: Some(requires_texture_param),
        calc_roi: None,
        is_need_render_frame: None,
        process_audio: None,
        on_property_edited: None,
        on_property_restored: None,
        poll_writeback: None,
        vertex_wgsl: None,
        compute_wgsl: None,
        compute_dispatch: None,
        custom_render: None,
        setup_accelerator: Some(setup_accelerator),
    })
}

const _: neoutl_effect_api::EntryFn = neoutl_effect_entry;
rust_i18n::i18n!("../../../../i18n");
extern crate rust_i18n;
