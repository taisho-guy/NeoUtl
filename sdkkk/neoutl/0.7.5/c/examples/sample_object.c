/**
 * sample_object.c - Example NeoUtl Object Plugin in C
 *
 * Demonstrates how to create a custom timeline object with:
 * - Property groups and parameters
 * - Custom WGSL shader & vertex counts
 */

#include <neoutl/neoutl.h>
#include <string.h>

/* Parameter Schema */
static const neoutl_param_schema_t s_params[] = {
    {
        /* key: */          { (const uint8_t*)"size", 4 },
        /* label: */        { (const uint8_t*)"サイズ", 9 },
        /* kind: */         NEOUTL_PARAM_FLOAT,
        /* min: */          1.0f,
        /* max: */          1000.0f,
        /* step: */         1.0f,
        /* default_float: */100.0f,
        /* enum_options: */ { NULL, 0 }
    }
};

/* Property Group */
static const neoutl_property_group_t s_groups[] = {
    {
        /* group_id: */ { (const uint8_t*)NEOUTL_DEFAULT_PROPERTY_GROUP_ID, 7 },
        /* schema: */   { s_params, sizeof(s_params) / sizeof(s_params[0]) }
    }
};

/* Object Metadata */
static const neoutl_object_meta_t s_meta = {
    /* stable_id: */       "c_sample.object.box",
    /* name: */            "C Sample Box",
    /* dimensionality: */  NEOUTL_DIMENSIONALITY_2D,
    /* property_groups: */ { s_groups, sizeof(s_groups) / sizeof(s_groups[0]) }
};

/* Embedded WGSL Shader */
static const char s_wgsl[] =
    "@vertex\n"
    "fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {\n"
    "    var pos = array<vec2<f32>, 6>(\n"
    "        vec2<f32>(-0.5, -0.5),\n"
    "        vec2<f32>( 0.5, -0.5),\n"
    "        vec2<f32>(-0.5,  0.5),\n"
    "        vec2<f32>(-0.5,  0.5),\n"
    "        vec2<f32>( 0.5, -0.5),\n"
    "        vec2<f32>( 0.5,  0.5)\n"
    "    );\n"
    "    return vec4<f32>(pos[in_vertex_index], 0.0, 1.0);\n"
    "}\n"
    "\n"
    "@fragment\n"
    "fn fs_main() -> @location(0) vec4<f32> {\n"
    "    return vec4<f32>(0.2, 0.6, 1.0, 1.0);\n"
    "}\n";

static const neoutl_object_meta_t* object_meta(void) {
    return &s_meta;
}

static uint32_t object_vertex_count(void) {
    return 6; /* 2 triangles */
}

static neoutl_wgsl_source_t object_wgsl(void) {
    neoutl_wgsl_source_t src;
    src.ptr = s_wgsl;
    src.len = strlen(s_wgsl);
    return src;
}

static void object_render(const neoutl_render_context_t* ctx) {
    (void)ctx;
}

static const neoutl_object_vtable_t s_vtable = {
    /* meta: */              object_meta,
    /* vertex_count: */      object_vertex_count,
    /* wgsl: */              object_wgsl,
    /* render: */            object_render,
    /* read_ref_layer: */    NULL,
    /* setup_accelerator: */ NULL
};

/* Exported entry point */
NEOUTL_EXPORT const neoutl_object_vtable_t* neoutl_object_entry(void) {
    return &s_vtable;
}
