/**
 * sample_extension.c - Example NeoUtl Extension / AviUtl2 Common Plugin in C
 *
 * Demonstrates a native C extension plugin interacting with NeoUtl host services.
 */

#include <neoutl/neoutl.h>
#include <stdio.h>

static const neoutl_host_app_table_t* s_host = NULL;

static int plugin_init(const neoutl_host_app_table_t* host) {
    s_host = host;
    if (s_host && s_host->log_info) {
        s_host->log_info("[C Extension] Initialized successfully in NeoUtl!");
    }
    return 0;
}

static int plugin_exit(void) {
    if (s_host && s_host->log_info) {
        s_host->log_info("[C Extension] Unloaded.");
    }
    s_host = NULL;
    return 0;
}

static int plugin_command(const char* cmd_id) {
    if (s_host && s_host->show_toast) {
        char msg[256];
        snprintf(msg, sizeof(msg), "C Plugin executed command: %s", cmd_id ? cmd_id : "unknown");
        s_host->show_toast(msg);
    }
    return 0;
}

static const neoutl_common_plugin_table_t s_plugin_table = {
    /* struct_size: */ sizeof(neoutl_common_plugin_table_t),
    /* plugin_id: */   "com.example.c_sample_extension",
    /* name: */        "C Sample Extension",
    /* version: */     "1.0.0",
    /* author: */      "NeoUtl C SDK",
    /* description: */ "Sample native C extension plugin for NeoUtl",
    /* init: */        plugin_init,
    /* exit: */        plugin_exit,
    /* command: */     plugin_command
};

NEOUTL_EXPORT const neoutl_common_plugin_table_t* neoutl_c_extension_entry(void) {
    return &s_plugin_table;
}
