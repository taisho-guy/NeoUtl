#pragma once

#include <QMetaObject>
#include <QQuickItem>
#include <utils/Entity.h>

namespace filament {
class Engine;
class Renderer;
class Scene;
class View;
class Camera;
class SwapChain;
class Skybox;
} // namespace filament

namespace AviQtl::Rendering {

// QMLから "FilamentCanvas" として利用可能なFilament統合ウィジェット。
// Filamentの初期化・描画ループ・リサイズをカプセル化し、
// 外部(QML/ECS)にはsceneId/currentFrameプロパティのみを公開する。
//
// 実装メモ:
//   vendor/filament は Linux で X11 Window (XID) のみをサポートし、
//   Wayland ネイティブハンドルには対応していない。
//   そのため createSwapChain(uint32_t w, uint32_t h) のヘッドレス版を使用する。
//
// 注: QML登録は main.cpp で qmlRegisterType により手動実施する。
class FilamentCanvas : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(int sceneId READ sceneId WRITE setSceneId NOTIFY sceneIdChanged)
    Q_PROPERTY(int currentFrame READ currentFrame WRITE setCurrentFrame NOTIFY currentFrameChanged)

  public:
    explicit FilamentCanvas(QQuickItem *parent = nullptr);
    ~FilamentCanvas() override;

    int sceneId() const { return m_sceneId; }
    void setSceneId(int id);

    int currentFrame() const { return m_currentFrame; }
    void setCurrentFrame(int frame);

  signals:
    void sceneIdChanged(int id);
    void currentFrameChanged(int frame);

  protected:
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &value) override;

  private slots:
    void handleWindowChanged(QQuickWindow *win);
    void renderFrame();
    void destroyFilament();

  private:
    void initFilament();
    void updateViewport(int w, int h);
    void updateNativeSurfaceGeometry();

    int m_sceneId = -1;
    int m_currentFrame = 0;

    QQuickWindow *m_window = nullptr;
    QMetaObject::Connection m_beforeRenderingConnection;
    QMetaObject::Connection m_sceneGraphInvalidatedConnection;

    // Filament objects (Forward declared)
    filament::Engine *m_engine = nullptr;
    filament::Renderer *m_renderer = nullptr;
    filament::Scene *m_scene = nullptr;
    filament::View *m_view = nullptr;
    filament::Camera *m_camera = nullptr;
    filament::SwapChain *m_swapChain = nullptr;
    filament::Skybox *m_skybox = nullptr;

    // Utils::Entity handles
    utils::Entity m_cameraEntity;

    // Native surface pointer (e.g. CAMetalLayer* for Apple/Metal)
    void *m_nativeSurface = nullptr;
};

} // namespace AviQtl::Rendering
