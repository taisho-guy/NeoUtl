#include "filament_canvas.hpp"
#include <QDebug>

#include <QQuickWindow>
#include <QSGRendererInterface>
#include <filament/Camera.h>
#include <filament/Engine.h>
#include <filament/Renderer.h>
#include <filament/Scene.h>
#include <filament/Skybox.h>
#include <filament/SwapChain.h>
#include <filament/View.h>
#include <filament/Viewport.h>
#include <utils/EntityManager.h>

#if defined(Q_OS_MACOS)
#include <CoreVideo/CoreVideo.h>
#include <QuartzCore/CAMetalLayer.h>
#endif

namespace AviQtl::Rendering {

// ─────────────────────────────────────────────────────────────────────────────
// 設計メモ:
//   vendor/filament の SwapChain.h を確認した結果、このビルドがサポートする
//   Linux ネイティブウィンドウは "X11 Window (XID)" のみであり、
//   Wayland (wl_surface) への対応は含まれていない。
//
//   Wayland セッションで wl_surface* や { wl_display*, wl_surface* } 構造体を
//   createSwapChain(void*) に渡すと、Filament は XID として誤解釈し
//   "enumerate size error" でクラッシュする。
//
//   解決策: createSwapChain(uint32_t width, uint32_t height) の
//   ヘッドレス版を使用する。ヘッドレス SwapChain は OS ウィンドウハンドルを
//   一切必要とせず、GPU 上のオフスクリーンバッファとして機能する。
//   描画結果の QQuickWindow への合成は将来の実装フェーズで対応する。
// ─────────────────────────────────────────────────────────────────────────────

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
        m_beforeRenderingConnection = connect(
            win, &QQuickWindow::beforeRendering,
            this, &FilamentCanvas::renderFrame, Qt::DirectConnection);
        m_sceneGraphInvalidatedConnection = connect(
            win, &QQuickWindow::sceneGraphInvalidated,
            this, &FilamentCanvas::destroyFilament, Qt::DirectConnection);
    }
}

void FilamentCanvas::initFilament() {
    if (m_engine || !m_window)
        return;

    if (!m_window->isExposed())
        return;

    // アイテム・ウィンドウ両方のサイズが確定するまで待つ
    const int logicalW = static_cast<int>(width());
    const int logicalH = static_cast<int>(height());
    if (logicalW <= 0 || logicalH <= 0)
        return;

    const double dpr = m_window->devicePixelRatio();
    const uint32_t pw = static_cast<uint32_t>(logicalW * dpr);
    const uint32_t ph = static_cast<uint32_t>(logicalH * dpr);

    qDebug() << "[FilamentCanvas] Initializing headless SwapChain" << pw << "x" << ph;

#if defined(Q_OS_MACOS)
    m_engine = filament::Engine::create(filament::Engine::Backend::METAL);
#else
    m_engine = filament::Engine::create(filament::Engine::Backend::VULKAN);
#endif

    if (!m_engine) {
        qWarning() << "[FilamentCanvas] Engine::create() failed.";
        return;
    }

    // ヘッドレス SwapChain: OS ウィンドウハンドル不要。
    // vendor/filament の SwapChain.h が Wayland をサポートしないため
    // createSwapChain(void*) に wl_surface 系を渡すことは不可。
    m_swapChain = m_engine->createSwapChain(pw, ph);

    m_renderer = m_engine->createRenderer();
    m_scene = m_engine->createScene();
    m_view = m_engine->createView();

    m_cameraEntity = utils::EntityManager::get().create();
    m_camera = m_engine->createCamera(m_cameraEntity);

    m_view->setScene(m_scene);
    m_view->setCamera(m_camera);

    // トーンマッピングを無効化してクリアカラーを生のまま表示する
    m_view->setPostProcessingEnabled(false);

    // 仕様書に基づき、クリアカラーを紺色 (#001A33 相当) に設定
    m_skybox = filament::Skybox::Builder()
                   .color({0.0f, 0.1f, 0.2f, 1.0f})
                   .build(*m_engine);
    m_scene->setSkybox(m_skybox);

    updateViewport(logicalW, logicalH);

    qDebug() << "[FilamentCanvas] Headless SwapChain initialized successfully.";
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
        if (!m_engine)
            return;
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

    // ジオメトリ変更時に SwapChain を再生成する (ヘッドレスは固定サイズのため)
    if (m_engine && newGeometry.width() > 0 && newGeometry.height() > 0) {
        if (newGeometry.size() != oldGeometry.size()) {
            const double dpr = m_window ? m_window->devicePixelRatio() : 1.0;
            const uint32_t pw = static_cast<uint32_t>(newGeometry.width() * dpr);
            const uint32_t ph = static_cast<uint32_t>(newGeometry.height() * dpr);

            m_engine->destroy(m_swapChain);
            m_swapChain = m_engine->createSwapChain(pw, ph);
            qDebug() << "[FilamentCanvas] SwapChain resized to" << pw << "x" << ph;
        }
    }

#if defined(Q_OS_MACOS)
    updateNativeSurfaceGeometry();
#endif
}

void FilamentCanvas::updateViewport(int w, int h) {
    if (!m_engine || !m_view || !m_camera || w <= 0 || h <= 0)
        return;

    const double dpr = m_window ? m_window->devicePixelRatio() : 1.0;
    const uint32_t pw = static_cast<uint32_t>(w * dpr);
    const uint32_t ph = static_cast<uint32_t>(h * dpr);

    m_view->setViewport({0, 0, pw, ph});

    // AviUtl互換の正投影 (Z=0をピクセル等倍)
    m_camera->setProjection(
        filament::Camera::Projection::ORTHO, 0, (double)w, (double)h, 0, -1.0, 1.0);
}

void FilamentCanvas::updateNativeSurfaceGeometry() {
#if defined(Q_OS_MACOS)
    if (m_nativeSurface) {
        // Metal Layer のリサイズ同期
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
