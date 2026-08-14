#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <libavutil/buffer.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vulkan.h>

static char **copy_extension_names(const char *const *names, int count) {
    if (count <= 0) {
        return NULL;
    }
    char **out = (char **)av_malloc_array((size_t)count, sizeof(char *));
    if (!out) {
        return NULL;
    }
    for (int i = 0; i < count; i++) {
        out[i] = av_strdup(names[i]);
    }
    return out;
}

int neoutl_vk_configure_device_ctx(
    AVBufferRef *av_hw_device_ctx,
    PFN_vkGetInstanceProcAddr get_proc_addr,
    uint64_t instance,
    uint64_t phys_dev,
    uint64_t act_dev,
    unsigned int queue_family_index,
    const char *const *enabled_inst_extensions,
    int nb_enabled_inst_extensions,
    const char *const *enabled_dev_extensions,
    int nb_enabled_dev_extensions)
{
    if (!av_hw_device_ctx || !av_hw_device_ctx->data) {
        return -1;
    }

    AVHWDeviceContext *hwctx = (AVHWDeviceContext *)av_hw_device_ctx->data;
    AVVulkanDeviceContext *vk_ctx = (AVVulkanDeviceContext *)hwctx->hwctx;
    if (!vk_ctx) {
        return -2;
    }

    vk_ctx->get_proc_addr = get_proc_addr;
    vk_ctx->inst = (VkInstance)(uintptr_t)instance;
    vk_ctx->phys_dev = (VkPhysicalDevice)(uintptr_t)phys_dev;
    vk_ctx->act_dev = (VkDevice)(uintptr_t)act_dev;

    vk_ctx->nb_qf = 0;
    vk_ctx->qf[vk_ctx->nb_qf].idx = (int)queue_family_index;
    vk_ctx->qf[vk_ctx->nb_qf].num = 1;
    vk_ctx->qf[vk_ctx->nb_qf].flags =
        VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT | VK_QUEUE_TRANSFER_BIT;
    vk_ctx->nb_qf++;

    // FFmpeg側は実際にVkInstance/VkDevice生成時に有効化された拡張の一覧を
    // 把握する手段を持たない(既存デバイスの再ラップのため)。呼び出し元が
    // 生成時点で保持していた一覧をそのまま渡し、ここで複製して設定する。
    // これが未設定(count=0)のままだと、FFmpegはDRM/external_memory系の
    // 相互運用能力を「無効」と誤認し、ゼロコピー導出経路(VAAPI→Vulkan等)を
    // 常時拒否する。
    vk_ctx->enabled_inst_extensions =
        (const char *const *)copy_extension_names(enabled_inst_extensions, nb_enabled_inst_extensions);
    vk_ctx->nb_enabled_inst_extensions = nb_enabled_inst_extensions;

    vk_ctx->enabled_dev_extensions =
        (const char *const *)copy_extension_names(enabled_dev_extensions, nb_enabled_dev_extensions);
    vk_ctx->nb_enabled_dev_extensions = nb_enabled_dev_extensions;

    return 0;
}

int neoutl_vk_frame_query_image0(
    void *av_vk_frame,
    uint64_t *out_image0,
    int *out_layout0)
{
    if (!av_vk_frame || !out_image0 || !out_layout0) {
        return -1;
    }

    AVVkFrame *vk_frame = (AVVkFrame *)av_vk_frame;
    *out_image0 = (uint64_t)(uintptr_t)vk_frame->img[0];
    *out_layout0 = (int)vk_frame->layout[0];
    return 0;
}
