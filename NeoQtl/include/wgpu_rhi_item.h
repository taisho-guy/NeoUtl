#pragma once

#include <QtQuick/QQuickRhiItem>
#include <rhi/qrhi.h>
#include <rhi/qrhi_platform.h>

class WgpuRhiItem : public QQuickRhiItem {
    Q_OBJECT
public:
    explicit WgpuRhiItem(QQuickItem *parent = nullptr);
    QQuickRhiItemRenderer *createRenderer() override;
};

class WgpuRhiRenderer : public QQuickRhiItemRenderer {
public:
    WgpuRhiRenderer();
    ~WgpuRhiRenderer() override;

protected:
    void initialize(QRhiCommandBuffer *cb) override;
    void synchronize(QQuickRhiItem *item) override;
    void render(QRhiCommandBuffer *cb) override;

private:
    std::size_t rust_context_id_;
    bool device_bound_;
};

void register_wgpu_rhi_item();
