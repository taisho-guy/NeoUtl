#ifndef NEOUTL_EXTENSION_H
#define NEOUTL_EXTENSION_H

#include "neoutl_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

#define NEOUTL_EXTENSION_ENTRY_SYMBOL "neoutl_extension_create"
#define NEOUTL_EXTENSION_API_VERSION 1

/* AviUtl2 C ABI Compatibility Layer */
typedef void* aviutl2_object_handle_t;
typedef void* aviutl2_effect_handle_t;

typedef struct {
    int layer;
    int start;
    int end;
} aviutl2_object_layer_frame_t;

typedef struct {
    int video_track_num;
    int audio_track_num;
    double total_time;
    int width;
    int height;
} aviutl2_media_info_t;

typedef struct {
    const uint16_t* mode;
    double* param;
    int param_num;
    bool accelerate;
    bool decelerate;
    bool twopoint;
    bool timecontrol;
    int group_num;
    int group_index;
} aviutl2_track_info_t;

typedef struct {
    const uint16_t* mode;
    int* param;
    int param_num;
} aviutl2_check_info_t;

typedef struct {
    void* exdata_ptr;
    int exdata_use;
    int exdata_size;
} aviutl2_exdata_info_t;

typedef struct {
    const char* font_name;
    int font_size;
    int weight;
    bool italic;
    bool underline;
} aviutl2_font_info_t;

typedef struct {
    int struct_size;
    const char* app_name;
    int version;
    void (*show_toast)(const char* message);
    void (*log_info)(const char* message);
    void (*log_error)(const char* message);
} neoutl_host_app_table_t;

typedef struct {
    int struct_size;
    int (*get_current_frame)(void);
    void (*set_current_frame)(int frame);
    int (*get_scene_count)(void);
    int (*get_current_scene)(void);
    void (*set_current_scene)(int scene_id);
    int (*get_object_count)(void);
    int (*create_text_object)(const char* text, int layer, int start, int duration);
    int (*create_shape_object)(const char* shape_type, int layer, int start, int duration);
    void (*delete_object)(int object_id);
    void (*move_object)(int object_id, int layer, int start);
} neoutl_edit_handle_t;

typedef struct {
    int struct_size;
    const char* name;
    int start_frame;
    int end_frame;
    int layer_count;
} neoutl_edit_section_t;

typedef struct {
    int struct_size;
    const char* file_path;
    int (*load)(const char* path);
    int (*save)(const char* path);
} neoutl_project_file_t;

typedef struct {
    int struct_size;
    const char* plugin_id;
    const char* name;
    const char* version;
    const char* author;
    const char* description;
    int (*init)(const neoutl_host_app_table_t* host);
    int (*exit)(void);
    int (*command)(const char* cmd_id);
} neoutl_common_plugin_table_t;

#ifdef __cplusplus
}
#endif

#endif /* NEOUTL_EXTENSION_H */
