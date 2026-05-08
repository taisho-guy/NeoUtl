#pragma once
#include <QQuickItem>
#include <QQuickWindow>
#include <filament/Camera.h>
#include <filament/Engine.h>
#include <filament/Renderer.h>
#include <filament/Scene.h>
#include <filament/SwapChain.h>
#include <filament/View.h>
#include <utils/EntityManager.h>

namespace AviQtl::Rendering {

// QMLから "FilamentCanvas" として利用可能なFilament統合ウィジェット。
// Filamentの初期化・描画ループ・リサイズをカプセル化し、
// 外部(QML/ECS)にはsceneId/currentFrameプロパティのみを公開する。
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
    void renderFrame();

  private:
    void handleWindowChanged(QQuickWindow *win);
    void initFilament();
    void destroyFilament();
    void updateViewport(int w, int h);

    int m_sceneId = -1;
    int m_currentFrame = 0;
    QQuickWindow *m_window = nullptr;
    QMetaObject::Connection m_beforeRenderingConnection;
    QMetaObject::Connection m_sceneGraphInvalidatedConnection;

    filament::Engine *m_engine = nullptr;
    filament::Renderer *m_renderer = nullptr;
    filament::Scene *m_scene = nullptr;
    filament::Camera *m_camera = nullptr;
    filament::View *m_view = nullptr;
    filament::SwapChain *m_swapChain = nullptr;
    utils::Entity m_cameraEntity{};
};

} // namespace AviQtl::Rendering
