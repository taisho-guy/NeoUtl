/**
 * sample_effect.c - Example NeoUtl Effect Plugin in C
 *
 * Demonstrates how to create a GPU-accelerated video filter with:
 * - Dynamic parameter schema (float slider)
 * - WGSL shader string
 * - Uniform buffer packing
 */

#include <neoutl/neoutl.h>
#include <string.h>

/* Parameter Schema definition */
static const neoutl_effect_param_schema_t s_params[] = {
    {
        /* key: */          { (const uint8_t*)"factor", 6 },
        /* label: */        { (const uint8_t*)"反転率", 9 },
        /* kind: */         NEOUTL_PARAM_FLOAT,
        /* min: */          0.0f,
        /* max: */          1.0f,
        /* step: */         0.01f,
        /* default_float: */1.0f,
        /* enum_options: */ { NULL, 0 }
    }
};

/* Effect Metadata */
static const neoutl_effect_meta_t s_meta = {
    /* id: */                     "c_sample_invert",
    /* name: */                   "C Sample Invert",
    /* category: */               "Color",
    /* param_schema: */           { s_params, sizeof(s_params) / sizeof(s_params[0]) },
    /* kind: */                   NEOUTL_EFFECT_IMAGE,
    /* author: */                 { (const uint8_t*)"NeoUtl C SDK", 15 },
    /* description: */            { (const uint8_t*)"A sample color inversion effect written in C", 43 },
    /* uuid: */                   { (const uint8_t*)"c_sample_invert", 15 },
    /* is_dummy: */               0,
    /* use_composition_camera: */ 0
};

/* Embedded WGSL Fragment Shader */
static const char s_wgsl[] =
    "struct Uniforms {\n"
    "    factor: f32,\n"
    "};\n"
    "@group(0) @binding(0) var<uniform> u: Uniforms;\n"
    "@group(0) @binding(1) var t_diffuse: texture_2d<f32>;\n"
    "@group(0) @binding(2) var s_diffuse: sampler;\n"
    "\n"
    "@fragment\n"
    "fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {\n"
    "    let color = textureSample(t_diffuse, s_diffuse, uv);\n"
    "    let inverted = vec3<f32>(1.0 - color.r, 1.0 - color.g, 1.0 - color.b);\n"
    "    return vec4<f32>(mix(color.rgb, inverted, u.factor), color.a);\n"
    "}\n";

static const neoutl_effect_meta_t* effect_meta(void) {
    return &s_meta;
}

static neoutl_wgsl_source_t effect_wgsl(void) {
    neoutl_wgsl_source_t src;
    src.ptr = s_wgsl;
    src.len = strlen(s_wgsl);
    return src;
}

static uint32_t effect_uniform_size(void) {
    return neoutl_uniform_size_std(1);
}

static void effect_pack_uniform(const float* params_ptr, uint32_t count, uint8_t* out_ptr) {
    neoutl_pack_uniform_std(params_ptr, count, out_ptr);
}

static const neoutl_effect_vtable_t s_vtable = {
    /* meta: */                   effect_meta,
    /* wgsl: */                   effect_wgsl,
    /* uniform_size: */           effect_uniform_size,
    /* pack_uniform: */           effect_pack_uniform,
    /* requires_texture_param: */ NULL,
    /* calc_roi: */               NULL,
    /* is_need_render_frame: */   NULL,
    /* process_audio: */          NULL,
    /* on_property_edited: */     NULL,
    /* on_property_restored: */   NULL,
    /* poll_writeback: */         NULL,
    /* setup_accelerator: */      NULL
};

/* Exported entry point */
NEOUTL_EXPORT const neoutl_effect_vtable_t* neoutl_effect_entry(void) {
    return &s_vtable;
}
