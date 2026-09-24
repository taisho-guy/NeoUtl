#ifndef NEOUTL_OBJECT_H
#define NEOUTL_OBJECT_H

#include "neoutl_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

#define NEOUTL_DEFAULT_PROPERTY_GROUP_ID "default"
#define NEOUTL_UNIT_SIZE_PX 200.0f

typedef struct {
    neoutl_str_ref_t group_id;
    neoutl_ffi_slice_t schema; /* slice of neoutl_param_schema_t */
} neoutl_property_group_t;

typedef struct {
    const char* stable_id;
    const char* name;
    uint8_t dimensionality; /* neoutl_dimensionality_t */
    neoutl_ffi_slice_t property_groups; /* slice of neoutl_property_group_t */
} neoutl_object_meta_t;

typedef struct {
    uint32_t version;
    void* render_pass_ptr;
    const void* bind_group_ptr;
    uint32_t vertex_count;
    float mvp_matrix[16];
    float opacity;
    bool depth_enabled;
    const void* ref_layer_texture_ptr;
    uint32_t ref_layer_texture_count;
} neoutl_render_context_t;

typedef struct {
    const neoutl_object_meta_t* (*meta)(void);
    uint32_t (*vertex_count)(void);
    neoutl_wgsl_source_t (*wgsl)(void);
    void (*render)(const neoutl_render_context_t* ctx);
    const void* (*read_ref_layer)(const neoutl_render_context_t* ctx, uint32_t index);
    uint32_t (*setup_accelerator)(const neoutl_accelerator_handle_t* accelerator);
} neoutl_object_vtable_t;

#define NEOUTL_OBJECT_ENTRY_SYMBOL "neoutl_object_entry"
typedef const neoutl_object_vtable_t* (*neoutl_object_entry_fn)(void);

/* Prototype for plugin implementation */
NEOUTL_EXPORT const neoutl_object_vtable_t* neoutl_object_entry(void);

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_OBJECT_H */
