#ifndef NEOUTL_EXPRESSION_H
#define NEOUTL_EXPRESSION_H

#include "neoutl_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    float (*get_property)(size_t object_id, neoutl_str_ref_t prop_name, float fallback);
    double (*get_time_seconds)(void);
    int32_t (*get_frame)(void);
    float (*get_fps)(void);
    int32_t (*get_object_layer)(size_t object_id);
} neoutl_expression_host_vtable_t;

typedef struct {
    neoutl_str_ref_t id;
    neoutl_str_ref_t name;
    neoutl_str_ref_t version;
} neoutl_expression_engine_meta_t;

typedef struct {
    size_t object_id;
    int32_t frame;
    double time_seconds;
    float current_value;
} neoutl_expression_eval_context_t;

typedef struct {
    const neoutl_expression_engine_meta_t* (*meta)(void);
    void (*bind_host)(const neoutl_expression_host_vtable_t* host);
    uint64_t (*compile)(neoutl_str_ref_t script);
    float (*evaluate)(uint64_t handle, const neoutl_expression_eval_context_t* ctx);
    void (*release)(uint64_t handle);
} neoutl_expression_engine_vtable_t;

#define NEOUTL_EXPRESSION_ENGINE_ENTRY_SYMBOL "neoutl_expression_engine_entry"
typedef const neoutl_expression_engine_vtable_t* (*neoutl_expression_engine_entry_fn)(void);

/* Prototype for plugin implementation */
NEOUTL_EXPORT const neoutl_expression_engine_vtable_t* neoutl_expression_engine_entry(void);

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_EXPRESSION_H */
