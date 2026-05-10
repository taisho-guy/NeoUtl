#include "filament_canvas.hpp"
#include <QDebug>
#include <QQuickWindow>
#include <filament/Camera.h>
#include <filament/Engine.h>
#include <filament/Renderer.h>
#include <filament/Scene.h>
#include <filament/Skybox.h>
#include <filament/SwapChain.h>
#include <filament/View.h>
#include <filament/Viewport.h>
#include <utils/EntityManager.h>

#if defined(__APPLE__)
#include <CoreVideo/CoreVideo.h>
#include <QtGui/qpa/qplatformwindow_p.h>
#include <QuartzCore/CAMetalLayer.h>
#endif

namespace AviQtl::Rendering {

FilamentCanvas::FilamentCanvas(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
    connect(this, &QQuickItem::windowChanged, this, &FilamentCanvas::handleWindowChanged);
}

FilamentCanvas::~FilamentCanvas() { destroyFilament(); }

void FilamentCanvas::setSceneId(int id) {
    if (m_sceneId == id)
        return;
    m_sceneId = id;
    emit sceneIdChanged(id);
}

void FilamentCanvas::setCurrentFrame(int frame) {
    if (m_currentFrame == frame)
        return;
    m_currentFrame = frame;
    emit currentFrameChanged(frame);
    update();
}

void FilamentCanvas::handleWindowChanged(QQuickWindow *win) {
    if (win) {
        m_window = win;
        m_beforeRenderingConnection = connect(win, &QQuickWindow::beforeRendering, this, &FilamentCanvas::renderFrame, Qt::DirectConnection);
        m_sceneGraphInvalidatedConnection = connect(win, &QQuickWindow::sceneGraphInvalidated, this, &FilamentCanvas::destroyFilament, Qt::DirectConnection);
    }
}

void FilamentCanvas::initFilament() {
    if (m_engine || !m_window)
        return;

    // ウィンドウハンドルの取得
    WId wid = m_window->winId();

    // デバッグログの追加
    qDebug() << "[FilamentCanvas] Attempting init. WId:" << Qt::hex << wid << Qt::dec << "Width:" << width() << "Height:" << height();

    // Qtが内部的なダミーID（0x40000001等）を返している間は初期化を待機
    if (wid == 0 || wid > 0x40000000) {
        qDebug() << "[FilamentCanvas] Waiting for valid native window handle...";
        return;
    }

    void *nativeWindow = reinterpret_cast<void *>(wid);
    qDebug() << "[FilamentCanvas] Native handle verified. Initializing Filament engine...";

#if defined(__APPLE__)
    // macOS/Metal のセットアップ
    m_engine = filament::Engine::create(filament::Engine::Backend::METAL);
    // CAMetalLayer の注入処理などは platform 依存のため、ここでは簡易化
    // 実際には updateNativeSurfaceGeometry() でレイヤーの調整を行う
#else
    m_engine = filament::Engine::create(filament::Engine::Backend::VULKAN);
#endif

    if (!m_engine)
        return;

    m_swapChain = m_engine->createSwapChain(nativeWindow);
    m_renderer = m_engine->createRenderer();
    m_scene = m_engine->createScene();
    m_view = m_engine->createView();

    // Entity の作成 (EntityManager の include が必要な箇所)
    m_cameraEntity = utils::EntityManager::get().create();
    m_camera = m_engine->createCamera(m_cameraEntity);

    m_view->setScene(m_scene);
    m_view->setCamera(m_camera);

    // 【重要】トーンマッピングを無効化して、クリアカラーを「生」で表示する
    // これにより、ライティング設定がなくても指定した色がそのまま画面に出る
    m_view->setPostProcessingEnabled(false);

    // 仕様書に基づき、クリアカラーを紺色 (#001A33 相当) に設定
    m_skybox = filament::Skybox::Builder().color({0.0f, 0.1f, 0.2f, 1.0f}).build(*m_engine);
    m_scene->setSkybox(m_skybox);

    updateViewport(width(), height());
}

void FilamentCanvas::destroyFilament() {
    if (!m_engine)
        return;

    if (m_camera) {
        m_engine->destroyCameraComponent(m_cameraEntity);
        utils::EntityManager::get().destroy(m_cameraEntity);
        m_camera = nullptr;
    }

    m_engine->destroy(m_skybox);
    m_engine->destroy(m_renderer);
    m_engine->destroy(m_view);
    m_engine->destroy(m_scene);
    m_engine->destroy(m_swapChain);

    filament::Engine::destroy(&m_engine);
    m_engine = nullptr;
}

void FilamentCanvas::renderFrame() {
    if (!m_engine) {
        initFilament();
    }

    if (m_renderer && m_swapChain && m_view) {
        if (m_renderer->beginFrame(m_swapChain)) {
            m_renderer->render(m_view);
            m_renderer->endFrame();
        }
    }
}

void FilamentCanvas::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    updateViewport(newGeometry.width(), newGeometry.height());
#if defined(__APPLE__)
    updateNativeSurfaceGeometry();
#endif
}

void FilamentCanvas::updateViewport(int w, int h) {
    if (!m_engine || !m_view || !m_camera || w <= 0 || h <= 0)
        return;

    const double dpr = m_window ? m_window->devicePixelRatio() : 1.0;
    const uint32_t width = static_cast<uint32_t>(w * dpr);
    const uint32_t height = static_cast<uint32_t>(h * dpr);

    m_view->setViewport({0, 0, width, height});

    // AviUtl互換の正投影 (Z=0をピクセル等倍)
    m_camera->setProjection(filament::Camera::Projection::ORTHO, 0, (double)w, (double)h, 0, -1.0, 1.0);
}

void FilamentCanvas::updateNativeSurfaceGeometry() {
#if defined(__APPLE__)
    if (m_nativeSurface) {
        // Metal Layer のリサイズ同期
        // [CATransaction begin]; ... [CATransaction commit];
    }
#endif
}

void FilamentCanvas::itemChange(ItemChange change, const ItemChangeData &value) {
    QQuickItem::itemChange(change, value);
    if (change == ItemVisibleHasChanged && !value.boolValue) {
        // 非表示時の最適化が必要ならここに記述
    }
}

} // namespace AviQtl::Rendering