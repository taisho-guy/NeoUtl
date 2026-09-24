#ifndef NEOUTL_EASING_H
#define NEOUTL_EASING_H

#include "neoutl_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int32_t frame;
    float value;
    const uint8_t* payload_ptr;
    size_t payload_len;
} neoutl_keyframe_t;

typedef struct {
    neoutl_str_ref_t id;
    neoutl_str_ref_t name;
} neoutl_easing_engine_meta_t;

typedef enum {
    NEOUTL_EDIT_RESULT_CANCEL  = 0,
    NEOUTL_EDIT_RESULT_SUCCESS = 1
} neoutl_edit_result_code_t;

typedef struct {
    uint32_t code; /* neoutl_edit_result_code_t */
    neoutl_keyframe_t* keyframes_ptr;
    size_t count;
} neoutl_edit_result_t;

typedef struct {
    const neoutl_easing_engine_meta_t* (*meta)(void);
    float (*evaluate)(
        const neoutl_keyframe_t* keyframes_ptr,
        size_t count,
        int32_t frame,
        float fallback
    );
    void (*open_editor_window)(
        const void* host_window_handle,
        const neoutl_keyframe_t* keyframes_ptr,
        size_t count,
        void (*on_complete)(void* user_data, neoutl_edit_result_t result),
        void* user_data
    );
    void (*serialize)(
        const neoutl_keyframe_t* keyframes_ptr,
        size_t count,
        uint8_t** out_ptr,
        size_t* out_len
    );
    void (*deserialize)(
        const uint8_t* bytes_ptr,
        size_t len,
        neoutl_keyframe_t** out_keyframes,
        size_t* out_count
    );
    void (*free_bytes)(uint8_t* ptr, size_t len);
    void (*free_keyframes)(neoutl_keyframe_t* ptr, size_t count);
} neoutl_easing_engine_vtable_t;

#define NEOUTL_EASING_ENGINE_ENTRY_SYMBOL "neoutl_easing_engine_entry"
typedef const neoutl_easing_engine_vtable_t* (*neoutl_easing_engine_entry_fn)(void);

/* Prototype for plugin implementation */
NEOUTL_EXPORT const neoutl_easing_engine_vtable_t* neoutl_easing_engine_entry(void);

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_EASING_H */
