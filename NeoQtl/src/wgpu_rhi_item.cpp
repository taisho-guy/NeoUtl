#include "wgpu_rhi_item.h"
#include "NeoQtl/src/ffi.cxx.h"

#include <QtQml/qqml.h>
#include <QtGui/QVulkanInstance>

WgpuRhiItem::WgpuRhiItem(QQuickItem *parent)
    : QQuickRhiItem(parent) {
}

QQuickRhiItemRenderer *WgpuRhiItem::createRenderer() {
    return new WgpuRhiRenderer();
}

WgpuRhiRenderer::WgpuRhiRenderer()
    : rust_context_id_(wgpu_renderer_create()), device_bound_(false) {
}

WgpuRhiRenderer::~WgpuRhiRenderer() {
    wgpu_renderer_destroy(rust_context_id_);
}

void WgpuRhiRenderer::initialize(QRhiCommandBuffer *) {
    if (device_bound_) return;

    QRhi *rhi_instance = rhi();
    NativeDeviceHandles handles{};

    switch (rhi_instance->backend()) {
    case QRhi::Vulkan: {
        auto *native = static_cast<const QRhiVulkanNativeHandles *>(rhi_instance->nativeHandles());
        handles.backend = 0;
        handles.instance = reinterpret_cast<quint64>(native->inst ? native->inst->vkInstance() : nullptr);
        handles.physical_device = reinterpret_cast<quint64>(native->physDev);
        handles.device = reinterpret_cast<quint64>(native->dev);
        handles.queue = reinterpret_cast<quint64>(native->gfxQueue);
        handles.queue_family_index = static_cast<quint32>(native->gfxQueueFamilyIdx);
        break;
    }
    case QRhi::Metal: {
#ifdef Q_OS_MACOS
        auto *native = static_cast<const QRhiMetalNativeHandles *>(rhi_instance->nativeHandles());
        handles.backend = 1;
        handles.device = reinterpret_cast<quint64>(native->dev);
        handles.queue = reinterpret_cast<quint64>(native->cmdQueue);
#endif
        break;
    }
    case QRhi::D3D12: {
#ifdef Q_OS_WIN
        auto *native = static_cast<const QRhiD3D12NativeHandles *>(rhi_instance->nativeHandles());
        handles.backend = 2;
        handles.device = reinterpret_cast<quint64>(native->dev);
        handles.queue = reinterpret_cast<quint64>(native->commandQueue);
#endif
        break;
    }
    default:
        return;
    }

    wgpu_renderer_bind_device(rust_context_id_, handles);
    device_bound_ = true;
}

void WgpuRhiRenderer::synchronize(QQuickRhiItem *) {
}

void WgpuRhiRenderer::render(QRhiCommandBuffer *) {
    QRhiTexture *color = colorTexture();
    if (!color) return;

    const QRhiTexture::NativeTexture native = color->nativeTexture();
    NativeTextureHandle texture{};
    texture.object = native.object;
    texture.layout_or_state = static_cast<quint32>(native.layout);
    texture.width = color->pixelSize().width();
    texture.height = color->pixelSize().height();

    wgpu_renderer_bind_texture(rust_context_id_, texture);
    wgpu_renderer_render(rust_context_id_);
    update();
}

void register_wgpu_rhi_item() {
    qmlRegisterType<WgpuRhiItem>("NeoQtl", 1, 0, "WgpuRhiItem");
}
