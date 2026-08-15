#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <libavutil/buffer.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vaapi.h>
#include <libavutil/hwcontext_vulkan.h>
#include <va/va.h>

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

int neoutl_vk_frame_query_sync0(
    void *av_vk_frame,
    uint64_t *out_semaphore,
    uint64_t *out_wait_value)
{
    if (!av_vk_frame || !out_semaphore || !out_wait_value) {
        return -1;
    }

    AVVkFrame *vk_frame = (AVVkFrame *)av_vk_frame;
    *out_semaphore = (uint64_t)(uintptr_t)vk_frame->sem[0];
    *out_wait_value = vk_frame->sem_value[0];
    return 0;
}

int neoutl_vk_frame_signal_sync0(
    void *av_vk_frame,
    uint64_t new_value)
{
    if (!av_vk_frame) {
        return -1;
    }

    AVVkFrame *vk_frame = (AVVkFrame *)av_vk_frame;
    vk_frame->sem_value[0] = new_value;
    return 0;
}

// AVVkFrame.semはVAAPI由来フレームのVulkanゼロコピー導出(vulkan_map_from_vaapi)経路で
// VA-APIデコード完了と結線されない(sem_valueが初期値のまま更新されない実装のため、
// Vulkan側のタイムラインセマフォ待機は実質的に即時充足され同期効果を持たない)。
// dma-buf経由の暗黙同期もradeonsi/RADVスタックではVA-API側の明示同期なしには
// 保証されないため、Vulkan導出(av_hwframe_map)前にVA-API層で直接完了を保証する。
int neoutl_vaapi_sync_surface(AVFrame *vaapi_frame)
{
    if (!vaapi_frame || !vaapi_frame->hw_frames_ctx || !vaapi_frame->data[3]) {
        return -1;
    }

    AVHWFramesContext *frames_ctx = (AVHWFramesContext *)vaapi_frame->hw_frames_ctx->data;
    if (!frames_ctx || !frames_ctx->device_ctx) {
        return -2;
    }

    AVVAAPIDeviceContext *vaapi_dev_ctx =
        (AVVAAPIDeviceContext *)frames_ctx->device_ctx->hwctx;
    if (!vaapi_dev_ctx) {
        return -3;
    }

    VASurfaceID surface = (VASurfaceID)(uintptr_t)vaapi_frame->data[3];
    VAStatus status = vaSyncSurface(vaapi_dev_ctx->display, surface);
    if (status != VA_STATUS_SUCCESS) {
        return -4;
    }

    return 0;
}
