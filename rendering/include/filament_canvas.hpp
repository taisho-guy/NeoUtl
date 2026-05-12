#pragma once

// ─────────────────────────────────────────────────────────────────────────────
// filament_canvas.hpp  (フェーズ2)
//
// 設計原則:
//   このヘッダは MOC / mocs_compilation.cpp にインクルードされるため、
//   Filament 内部ヘッダ (VulkanPlatform.h / bluevk/BlueVK.h) および
//   Vulkan 型 (VkImage 等) を一切露出させてはならない。
//
//   すべての Filament / Vulkan 依存は filament_canvas.cpp の内部に閉じ込め、
//   pimpl (Pointer to IMPLementation) パターンで完全隠蔽する。
//
// Phase: フェーズ2 (VulkanSharedContextQt 実装)
// ─────────────────────────────────────────────────────────────────────────────

#include <QMetaObject>
#include <QQuickItem>
#include <QSGSimpleTextureNode>

#include <atomic>
#include <cstdint>
#include <memory>

namespace AviQtl::Rendering {

// 前方宣言 — 完全定義は filament_canvas.cpp の内部にのみ存在する
struct FilamentCanvasImpl;

// ─────────────────────────────────────────────────────────────────────────────
// FilamentCanvas
//
// QML から "AviQtl.Rendering" モジュール経由で使用する Filament 描画アイテム。
//
//   import AviQtl.Rendering 1.0
//   FilamentCanvas { sceneId: 0; anchors.fill: parent }
//
// 内部アーキテクチャ (フェーズ2):
//   QtVulkanPlatform が Qt SceneGraph の VkDevice を Filament と共有する。
//   Filament はオフスクリーン VkImage に描画し、Qt SceneGraph は
//   QNativeInterface::QSGVulkanTexture::fromNative() でその VkImage を
//   QSGTexture としてラップして表示する (ゼロコピー)。
//
//   ┌─────────────────┐  同一 VkDevice  ┌──────────────────┐
//   │  Qt SG (Vulkan) │ ◀────────────▶ │ Filament (Vulkan) │
//   │  QQuickWindow   │                 │ QtVulkanPlatform  │
//   └────────┬────────┘                 └────────┬──────────┘
//            │ QSGVulkanTexture::fromNative()     │ VkImage
//            └────────────────────────────────────┘
// ─────────────────────────────────────────────────────────────────────────────
class FilamentCanvas : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT

    Q_PROPERTY(int sceneId READ sceneId WRITE setSceneId NOTIFY sceneIdChanged)
    Q_PROPERTY(int currentFrame READ currentFrame WRITE setCurrentFrame NOTIFY currentFrameChanged)

  public:
    explicit FilamentCanvas(QQuickItem *parent = nullptr);
    ~FilamentCanvas() override;

    int sceneId() const noexcept;
    void setSceneId(int id);

    int currentFrame() const noexcept;
    void setCurrentFrame(int frame);

  signals:
    void sceneIdChanged(int id);
    void currentFrameChanged(int frame);

  protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;

  private slots:
    void handleWindowChanged(QQuickWindow *win);
    void onBeforeRendering();
    void onSceneGraphInvalidated();

  private:
    // Filament / Vulkan のすべての実装詳細を隠蔽する pimpl
    std::unique_ptr<FilamentCanvasImpl> m_impl;

    int m_sceneId = -1;
    int m_currentFrame = 0;

    QQuickWindow *m_window = nullptr;
    QMetaObject::Connection m_beforeRenderingConn;
    QMetaObject::Connection m_sceneGraphInvalidatedConn;
    QMetaObject::Connection m_sgInitializedConn;

    std::atomic<bool> m_frameDirty{false};
    uint32_t m_targetW = 0;
    uint32_t m_targetH = 0;
};

} // namespace AviQtl::Rendering
