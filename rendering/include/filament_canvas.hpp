#pragma once

// filament_canvas.hpp  —  VulkanSharedContextQt
//
// 設計原則:
//   MOC / mocs_compilation.cpp にインクルードされるため、
//   Filament 内部ヘッダ (VulkanPlatform.h / bluevk/BlueVK.h) および
//   Vulkan 型 (VkImage 等) を一切露出させない。
//   すべての実装詳細は filament_canvas.cpp の pimpl に閉じ込める。

#include <QMetaObject>
#include <QQuickItem>
#include <QQuickWindow>

#include <atomic>
#include <memory>

namespace AviQtl::Rendering {

struct FilamentCanvasImpl; // 完全定義は filament_canvas.cpp のみ

class FilamentCanvas : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT

    Q_PROPERTY(int sceneId READ sceneId WRITE setSceneId NOTIFY sceneIdChanged)
    Q_PROPERTY(int currentFrame READ currentFrame WRITE setCurrentFrame NOTIFY currentFrameChanged)
    // プロジェクト解像度: Filament はこのサイズで固定レンダリングする
    // ウィンドウリサイズでは Qt SG が scale するだけで Filament は再生成しない
    Q_PROPERTY(int projectWidth READ projectWidth WRITE setProjectWidth NOTIFY projectWidthChanged)
    Q_PROPERTY(int projectHeight READ projectHeight WRITE setProjectHeight NOTIFY projectHeightChanged)

  public:
    explicit FilamentCanvas(QQuickItem *parent = nullptr);
    ~FilamentCanvas() override;

    int sceneId() const noexcept;
    int currentFrame() const noexcept;
    void setSceneId(int id);
    void setCurrentFrame(int frame);
    int projectWidth() const noexcept;
    int projectHeight() const noexcept;
    void setProjectWidth(int w);
    void setProjectHeight(int h);

  signals:
    void sceneIdChanged(int id);
    void currentFrameChanged(int frame);
    void projectWidthChanged(int w);
    void projectHeightChanged(int h);

  protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;

  private slots:
    void handleWindowChanged(QQuickWindow *win);
    void onBeforeRendering();
    void onSceneGraphInvalidated();

  private:
    std::unique_ptr<FilamentCanvasImpl> m_impl;

    int m_sceneId = -1;
    int m_currentFrame = 0;
    int m_renderWidth = 1920;
    int m_renderHeight = 1080;

    QQuickWindow *m_window = nullptr;
    QMetaObject::Connection m_beforeRenderingConn;
    QMetaObject::Connection m_sceneGraphInvalidatedConn;

    std::atomic<bool> m_frameDirty{false};
};

} // namespace AviQtl::Rendering
