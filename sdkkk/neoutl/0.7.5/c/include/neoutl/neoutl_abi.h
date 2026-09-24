#ifndef NEOUTL_ABI_H
#define NEOUTL_ABI_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Export macro */
#if defined(_WIN32) || defined(__CYGWIN__)
  #define NEOUTL_EXPORT __declspec(dllexport)
#else
  #define NEOUTL_EXPORT __attribute__((visibility("default")))
#endif

typedef enum {
    NEOUTL_DIMENSIONALITY_2D   = 0,
    NEOUTL_DIMENSIONALITY_3D   = 1,
    NEOUTL_DIMENSIONALITY_BOTH = 2
} neoutl_dimensionality_t;

typedef enum {
    NEOUTL_PARAM_FLOAT      = 0,
    NEOUTL_PARAM_BOOL       = 1,
    NEOUTL_PARAM_COLOR      = 2,
    NEOUTL_PARAM_ENUM       = 3,
    NEOUTL_PARAM_TEXT       = 4,
    NEOUTL_PARAM_FILE_PATH  = 5,
    NEOUTL_PARAM_TRACK      = 6,
    NEOUTL_PARAM_SEPARATOR  = 7,
    NEOUTL_PARAM_GROUP      = 8,
    NEOUTL_PARAM_FOLDER     = 9
} neoutl_param_kind_t;

typedef enum {
    NEOUTL_EFFECT_IMAGE = 0,
    NEOUTL_EFFECT_AUDIO = 1,
    NEOUTL_EFFECT_BOTH  = 2
} neoutl_effect_kind_t;

typedef struct {
    const uint8_t* ptr;
    size_t len;
} neoutl_str_ref_t;

static inline neoutl_str_ref_t neoutl_str_ref_make(const char* s) {
    neoutl_str_ref_t ref;
    if (s) {
        ref.ptr = (const uint8_t*)s;
        ref.len = strlen(s);
    } else {
        ref.ptr = NULL;
        ref.len = 0;
    }
    return ref;
}

static inline neoutl_str_ref_t neoutl_str_ref_empty(void) {
    neoutl_str_ref_t ref = { NULL, 0 };
    return ref;
}

typedef struct {
    const void* ptr;
    size_t len;
} neoutl_ffi_slice_t;

static inline neoutl_ffi_slice_t neoutl_ffi_slice_make(const void* ptr, size_t count) {
    neoutl_ffi_slice_t slice;
    slice.ptr = ptr;
    slice.len = count;
    return slice;
}

typedef neoutl_ffi_slice_t neoutl_wgsl_source_t;

typedef struct {
    float x;
    float y;
    float width;
    float height;
} neoutl_roi_t;

typedef struct {
    neoutl_str_ref_t key;
    neoutl_str_ref_t label;
    uint8_t kind; /* neoutl_param_kind_t */
    float min;
    float max;
    float step;
    float default_float;
    neoutl_str_ref_t enum_options;
} neoutl_param_schema_t;

typedef enum {
    NEOUTL_ACCELERATOR_BACKEND_NONE   = 0,
    NEOUTL_ACCELERATOR_BACKEND_METAL  = 1,
    NEOUTL_ACCELERATOR_BACKEND_VULKAN = 2,
    NEOUTL_ACCELERATOR_BACKEND_DX12   = 3
} neoutl_accelerator_backend_t;

#define NEOUTL_ACCELERATOR_HANDLE_CURRENT_VERSION 1

typedef struct {
    uint32_t version;
    uint32_t backend;
    const void* device_ptr;
    const void* queue_ptr;
} neoutl_accelerator_handle_t;

typedef struct {
    neoutl_str_ref_t key;
    float value;
} neoutl_property_writeback_t;

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_ABI_H */
