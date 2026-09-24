#ifndef NEOUTL_EFFECT_H
#define NEOUTL_EFFECT_H

#include "neoutl_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef neoutl_param_schema_t neoutl_effect_param_schema_t;

typedef struct {
    const char* id;
    const char* name;
    const char* category;
    neoutl_ffi_slice_t param_schema; /* slice of neoutl_effect_param_schema_t */
    uint8_t kind; /* neoutl_effect_kind_t */
    neoutl_str_ref_t author;
    neoutl_str_ref_t description;
    neoutl_str_ref_t uuid;
    uint8_t is_dummy;
    uint8_t use_composition_camera;
} neoutl_effect_meta_t;

typedef struct {
    const neoutl_effect_meta_t* (*meta)(void);
    neoutl_wgsl_source_t (*wgsl)(void);
    uint32_t (*uniform_size)(void);
    void (*pack_uniform)(const float* params_ptr, uint32_t count, uint8_t* out_ptr);
    uint32_t (*requires_texture_param)(void);

    neoutl_roi_t (*calc_roi)(
        neoutl_roi_t base,
        const float* params_ptr,
        uint32_t count,
        int64_t layer_time_us,
        float downsample_x,
        float downsample_y
    );

    uint32_t (*is_need_render_frame)(const float* params_ptr, uint32_t count, int64_t layer_time_us);

    uint32_t (*process_audio)(
        float* samples_ptr,
        uint32_t sample_count,
        const float* params_ptr,
        uint32_t param_count
    );

    void (*on_property_edited)(const float* params_ptr, uint32_t count);
    void (*on_property_restored)(const float* params_ptr, uint32_t count);
    uint32_t (*poll_writeback)(neoutl_property_writeback_t* out_ptr, uint32_t out_cap);
    uint32_t (*setup_accelerator)(const neoutl_accelerator_handle_t* accelerator);
} neoutl_effect_vtable_t;

#define NEOUTL_EFFECT_ENTRY_SYMBOL "neoutl_effect_entry"
typedef const neoutl_effect_vtable_t* (*neoutl_effect_entry_fn)(void);

static inline uint32_t neoutl_uniform_size_std(uint32_t count) {
    return ((count + 3) / 4) * 16;
}

static inline void neoutl_pack_uniform_std(const float* params, uint32_t count, uint8_t* out_ptr) {
    uint32_t total = neoutl_uniform_size_std(count);
    memset(out_ptr, 0, total);
    if (params && count > 0) {
        memcpy(out_ptr, params, count * sizeof(float));
    }
}

/* Prototype for plugin implementation */
NEOUTL_EXPORT const neoutl_effect_vtable_t* neoutl_effect_entry(void);

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_EFFECT_H */
