#include "filament_canvas.hpp"
#include <QQuickWindow>
#include <filament/Viewport.h>

#if defined(__APPLE__)
#include <QtGui/qpa/qplatformwindow_p.h>
#include <TargetConditionals.h>
#endif

namespace AviQtl::Rendering {

FilamentCanvas::FilamentCanvas(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
    connect(this, &QQuickItem::windowChanged, this, [this](QQuickWindow *win) { handleWindowChanged(win); });
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
}

void FilamentCanvas::handleWindowChanged(QQuickWindow *win) {
    if (m_window) {
        disconnect(m_beforeRenderingConnection);
        disconnect(m_sceneGraphInvalidatedConnection);
        m_beforeRenderingConnection = {};
        m_sceneGraphInvalidatedConnection = {};
    }

    m_window = win;
    if (win) {
        m_beforeRenderingConnection = connect(win, &QQuickWindow::beforeRendering, this, &FilamentCanvas::renderFrame, Qt::DirectConnection);
        m_sceneGraphInvalidatedConnection = connect(win, &QQuickWindow::sceneGraphInvalidated, this, &FilamentCanvas::destroyFilament, Qt::DirectConnection);
        win->update();
    } else {
        destroyFilament();
    }
}

void FilamentCanvas::initFilament() {
    if (m_engine)
        return;
    if (!m_window)
        return;
    void *nativeWindow = reinterpret_cast<void *>(m_window->winId());
#if defined(__APPLE__) && TARGET_OS_OSX
    if (auto *cocoaWindow = m_window->nativeInterface<QNativeInterface::Private::QCocoaWindow>())
        nativeWindow = cocoaWindow->contentLayer();
    m_engine = filament::Engine::create(filament::Engine::Backend::METAL);
#else
    m_engine = filament::Engine::create(filament::Engine::Backend::VULKAN);
#endif
    m_swapChain = m_engine->createSwapChain(nativeWindow);
    m_renderer = m_engine->createRenderer();
    m_scene = m_engine->createScene();
    m_view = m_engine->createView();
    auto &em = m_engine->getEntityManager();
    m_cameraEntity = em.create();
    m_camera = m_engine->createCamera(m_cameraEntity);
    m_view->setScene(m_scene);
    m_view->setCamera(m_camera);
    filament::Renderer::ClearOptions clearOpts;
    clearOpts.clearColor = {0.07f, 0.07f, 0.07f, 1.0f};
    clearOpts.clear = true;
    m_renderer->setClearOptions(clearOpts);
    const int w = static_cast<int>(width());
    const int h = static_cast<int>(height());
    if (w > 0 && h > 0)
        updateViewport(w, h);
}

void FilamentCanvas::destroyFilament() {
    if (!m_engine)
        return;
    m_engine->destroyCameraComponent(m_cameraEntity);
    m_engine->getEntityManager().destroy(m_cameraEntity);
    m_engine->destroy(m_view);
    m_engine->destroy(m_scene);
    m_engine->destroy(m_renderer);
    m_engine->destroy(m_swapChain);
    filament::Engine::destroy(&m_engine);
    m_engine = nullptr;
}

void FilamentCanvas::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (m_view && newGeometry.isValid())
        updateViewport(static_cast<int>(newGeometry.width()), static_cast<int>(newGeometry.height()));
}

void FilamentCanvas::itemChange(ItemChange change, const ItemChangeData &value) {
    QQuickItem::itemChange(change, value);
    if (change == ItemSceneChange && value.window)
        handleWindowChanged(value.window);
}

void FilamentCanvas::updateViewport(int w, int h) {
    m_view->setViewport({0, 0, static_cast<uint32_t>(w), static_cast<uint32_t>(h)});
    const double hw = w / 2.0, hh = h / 2.0;
    m_camera->setProjection(filament::Camera::Projection::ORTHO, -hw, hw, -hh, hh, 0.0, 10000.0);
}

void FilamentCanvas::renderFrame() {
    initFilament();
    if (!m_engine || !m_renderer || !m_swapChain || !m_view)
        return;
    if (m_renderer->beginFrame(m_swapChain)) {
        m_renderer->render(m_view);
        m_renderer->endFrame();
    }
}

} // namespace AviQtl::Rendering
