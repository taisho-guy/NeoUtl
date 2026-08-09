#include <stdint.h>

#include <libavutil/buffer.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vulkan.h>

int neoutl_vk_configure_device_ctx(
    AVBufferRef *av_hw_device_ctx,
    PFN_vkGetInstanceProcAddr get_proc_addr,
    uint64_t instance,
    uint64_t phys_dev,
    uint64_t act_dev,
    unsigned int queue_family_index)
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
