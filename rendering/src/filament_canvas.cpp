// filament_canvas.cpp — VulkanSharedContextQt
//
// アーキテクチャ方針:
//   Filament はプロジェクト解像度 (projectWidth x projectHeight) で固定レンダリング。
//   ウィンドウリサイズでは Qt SG が QSGSimpleTextureNode を scale するだけで
//   Filament 側のリソースは再生成しない。
//
//   Filament の headless SwapChain が描画した VkImage を
//   AviQtlVulkanPlatform が記録した backend SwapChain handle から取得し、
//   QSGVulkanTexture::fromNative() で Qt SG に渡す。
//
//   Filament の public SwapChain* と backend Platform::SwapChain* は別物なので、
//   MSVC では reinterpret_cast せず、Platform が作成した handle を保持する。

#include "filament_canvas.hpp"

#include <vulkan/vulkan.h>

#include <QDebug>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QSGSimpleTextureNode>
#include <QSGTexture>
#include <QVulkanInstance>

// BlueVK と最新の Vulkan SDK の互換性維持のためのパッチ
#if !defined(PFN_vkCmdSetDispatchParametersARM)
typedef void(VKAPI_PTR *PFN_vkCmdSetDispatchParametersARM)(VkCommandBuffer commandBuffer, const void *pData);
#endif

#include <backend/platforms/VulkanPlatform.h>
#if defined(_WIN32)
#include <backend/platforms/VulkanPlatformWindows.h>
#elif defined(__APPLE__)
#include <backend/platforms/VulkanPlatformApple.h>
#else
#include <backend/platforms/VulkanPlatformLinux.h>
#endif

#include <filament/Camera.h>
#include <filament/Engine.h>
#include <filament/RenderTarget.h>
#include <filament/Renderer.h>
#include <filament/Scene.h>
#include <filament/Skybox.h>
#include <filament/SwapChain.h>
#include <filament/Texture.h>
#include <filament/View.h>
#include <filament/Viewport.h>
#include <utils/EntityManager.h>

#include <mutex>

namespace AviQtl::Rendering {

// Filament v1.71.3 の Vulkan バックエンドが期待する外部共有コンテキストのレイアウト。
// ABI 不整合を防ぐため、フィールドの順序と型を厳密に維持する。
struct FilamentVulkanSharedContext {
    VkInstance instance = VK_NULL_HANDLE;
    VkPhysicalDevice physicalDevice = VK_NULL_HANDLE;
    VkDevice logicalDevice = VK_NULL_HANDLE;
    uint32_t graphicsQueueFamilyIndex = 0xFFFFFFFF;
    uint32_t graphicsQueueIndex = 0xFFFFFFFF;
    bool debugUtilsSupported = false;
    bool debugMarkersSupported = false;
    bool multiviewSupported = false;
};

namespace {

#if defined(_WIN32)
using AviQtlVulkanPlatformBase = filament::backend::VulkanPlatformWindows;
#elif defined(__APPLE__)
using AviQtlVulkanPlatformBase = filament::backend::VulkanPlatformApple;
#else
using AviQtlVulkanPlatformBase = filament::backend::VulkanPlatformLinux;
#endif

class AviQtlVulkanPlatform final : public AviQtlVulkanPlatformBase {
  public:
    SwapChainPtr createSwapChain(void *nativeWindow, uint64_t flags = 0, VkExtent2D extent = {0, 0}) override {
        auto *swapChain = AviQtlVulkanPlatformBase::createSwapChain(nativeWindow, flags, extent);
        if (!nativeWindow && extent.width > 0 && extent.height > 0) {
            std::lock_guard lock(m_mutex);
            m_headlessSwapChain = swapChain;
        }
        return swapChain;
    }

    void destroy(SwapChainPtr handle) override {
        {
            std::lock_guard lock(m_mutex);
            if (m_headlessSwapChain == handle)
                m_headlessSwapChain = nullptr;
        }
        AviQtlVulkanPlatformBase::destroy(handle);
    }

    SwapChainPtr headlessSwapChain() const {
        std::lock_guard lock(m_mutex);
        return m_headlessSwapChain;
    }

  private:
    mutable std::mutex m_mutex;
    SwapChainPtr m_headlessSwapChain = nullptr;
};

} // namespace

// ─────────────────────────────────────────────────────────────────────────────
// FilamentCanvasImpl  — pimpl
// ─────────────────────────────────────────────────────────────────────────────
struct FilamentCanvasImpl {
    // スタックローカルではなく、Engine の生存期間中維持される場所に置く。
    FilamentVulkanSharedContext sharedCtx;
    std::unique_ptr<AviQtlVulkanPlatform> platform;

    // Qt 側から受け取る Vulkan コンテキスト
    QVulkanInstance *qvkInstance = nullptr;
    VkInstance vkInstance = VK_NULL_HANDLE;
    VkPhysicalDevice physDev = VK_NULL_HANDLE;
    VkDevice dev = VK_NULL_HANDLE;
    uint32_t queueFamilyIdx = 0;
    uint32_t queueIdx = 0; // Qt が実際に使っているキュー index

    // Filament オブジェクト
    filament::Engine *engine = nullptr;
    filament::Renderer *renderer = nullptr;
    filament::Scene *scene = nullptr;
    filament::View *view = nullptr;
    filament::Camera *camera = nullptr;
    filament::Skybox *skybox = nullptr;
    filament::SwapChain *swapChain = nullptr;

    // オフスクリーン RenderTarget
    filament::RenderTarget *renderTarget = nullptr;
    filament::Texture *colorTex = nullptr;
    filament::Texture *depthTex = nullptr;

    utils::Entity cameraEntity;

    // QSGTexture ラッパー
    // Filament SwapChain の内部 VkImage を fromNative で包んだもの
    QSGTexture *sgTexture = nullptr;
    VkImage lastBoundImage = VK_NULL_HANDLE;

    // Filament 内部での描画解解像度
    std::atomic<uint32_t> renderW{1920};
    std::atomic<uint32_t> renderH{1080};

    std::atomic<bool> swapChainDirty{false};

    bool engineReady() const noexcept { return engine != nullptr; }
};

// ─────────────────────────────────────────────────────────────────────────────
// ctor / dtor
// ─────────────────────────────────────────────────────────────────────────────

FilamentCanvas::FilamentCanvas(QQuickItem *parent) : QQuickItem(parent), m_impl(std::make_unique<FilamentCanvasImpl>()) {
    setFlag(ItemHasContents, true);
    connect(this, &QQuickItem::windowChanged, this, &FilamentCanvas::handleWindowChanged);
}

FilamentCanvas::~FilamentCanvas() {
    if (m_window) {
        disconnect(m_beforeRenderingConn);
        disconnect(m_sceneGraphInvalidatedConn);
    }
    // sgTexture はレンダースレッドで生成・破棄する。通常は
    // sceneGraphInvalidated で破棄済みになるため、ここでは触らない。
}

// ─── プロパティ ───────────────────────────────────────────────────────────────

int FilamentCanvas::sceneId() const noexcept { return m_sceneId; }
int FilamentCanvas::currentFrame() const noexcept { return m_currentFrame; }
int FilamentCanvas::projectWidth() const noexcept { return m_renderWidth; }
int FilamentCanvas::projectHeight() const noexcept { return m_renderHeight; }

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
    m_frameDirty.store(true, std::memory_order_release);
    update();
}

void FilamentCanvas::setProjectWidth(int width) {
    if (m_renderWidth == width)
        return;
    m_renderWidth = width;

    auto *d = m_impl.get();
    d->renderW.store(static_cast<uint32_t>(width), std::memory_order_release);

    // プロジェクト解像度変更時はレンダースレッドで swapchain を再生成する
    if (d->engineReady()) {
        d->swapChainDirty.store(true, std::memory_order_release);
    }

    emit projectWidthChanged(width);
    update();
}

void FilamentCanvas::setProjectHeight(int height) {
    if (m_renderHeight == height)
        return;
    m_renderHeight = height;

    auto *d = m_impl.get();
    d->renderH.store(static_cast<uint32_t>(height), std::memory_order_release);

    if (d->engineReady()) {
        d->swapChainDirty.store(true, std::memory_order_release);
    }

    emit projectHeightChanged(height);
    update();
}

// ─── ウィンドウ接続 ───────────────────────────────────────────────────────────

void FilamentCanvas::handleWindowChanged(QQuickWindow *win) {
    if (m_window) {
        disconnect(m_beforeRenderingConn);
        disconnect(m_sceneGraphInvalidatedConn);
    }
    m_window = win;
    if (!win)
        return;

    m_beforeRenderingConn = connect(win, &QQuickWindow::beforeRendering, this, &FilamentCanvas::onBeforeRendering, Qt::DirectConnection);
    m_sceneGraphInvalidatedConn = connect(win, &QQuickWindow::sceneGraphInvalidated, this, &FilamentCanvas::onSceneGraphInvalidated, Qt::DirectConnection);

    win->setColor(Qt::transparent);
}

// ─── Filament 初期化 ──────────────────────────────────────────────────────────

static void initFilamentImpl(FilamentCanvasImpl *d, QQuickWindow *win) {
    if (d->engineReady())
        return;
    if (!win)
        return;

    // QSGRendererInterface から Vulkan リソースを取得
    QSGRendererInterface *rif = win->rendererInterface();
    if (!rif || rif->graphicsApi() != QSGRendererInterface::Vulkan) {
        qCritical("[FilamentCanvas] Vulkan SceneGraph が利用できません。");
        return;
    }

    auto *qvkInst = reinterpret_cast<QVulkanInstance *>(rif->getResource(win, QSGRendererInterface::VulkanInstanceResource));
    auto *physDevPtr = static_cast<VkPhysicalDevice *>(rif->getResource(win, QSGRendererInterface::PhysicalDeviceResource));
    auto *devPtr = static_cast<VkDevice *>(rif->getResource(win, QSGRendererInterface::DeviceResource));
    auto *queueFamilyIdxPtr = static_cast<uint32_t *>(rif->getResource(win, QSGRendererInterface::GraphicsQueueFamilyIndexResource));

    // Qt 6.0+ で追加された GraphicsQueueIndexResource
    // Qt が実際に使っているキューの index を取得する。
    // これがないと queueIdx = 0 にフォールバックする。
    uint32_t queueIdx = 0;
    if (auto *ptr = static_cast<uint32_t *>(rif->getResource(win, QSGRendererInterface::GraphicsQueueIndexResource))) {
        queueIdx = *ptr;
    }

    if (!qvkInst || !physDevPtr || !devPtr || !queueFamilyIdxPtr || *physDevPtr == VK_NULL_HANDLE || *devPtr == VK_NULL_HANDLE) {
        qCritical("[FilamentCanvas] Vulkan リソース取得失敗。SceneGraph 未初期化の可能性。");
        return;
    }

    d->qvkInstance = qvkInst;
    d->vkInstance = qvkInst->vkInstance();
    d->physDev = *physDevPtr;
    d->dev = *devPtr;
    d->queueFamilyIdx = *queueFamilyIdxPtr;
    d->queueIdx = queueIdx;

    qDebug("[FilamentCanvas] Vulkan コンテキスト取得完了。Filament を初期化します。");
    qDebug("[FilamentCanvas] queueFamily=%u queueIndex=%u", d->queueFamilyIdx, d->queueIdx);

    // スタックローカルに置いた sharedCtx を渡すと、build() がスレッドを起動した後に
    // スタックフレームが破棄され、Filament のレンダースレッドが解放済みメモリを
    // 参照して SIGSEGV を引き起こす。
    d->sharedCtx.instance = d->vkInstance;
    d->sharedCtx.physicalDevice = d->physDev;
    d->sharedCtx.logicalDevice = d->dev;
    d->sharedCtx.graphicsQueueFamilyIndex = d->queueFamilyIdx;
    // VulkanPlatform.h のコメント:
    //   "In the usual case, the client needs to allocate at least one more
    //    graphics queue for Filament, and this index is the param to pass
    //    into vkGetDeviceQueue."
    //
    // Qt が VkDevice を所有しているため、こちらからキューを追加確保できない。
    // Qt は beforeRendering シグナル発火時点でそのフレームのサブミットを完了している。
    // Filament も同じ DirectConnection シグナル内でサブミットするため、
    // 同一スレッド・直列実行が保証される。VkQueue への同時アクセスは発生しない。
    // よって Qt と同じ index を渡しても Vulkan spec 上の競合は生じない。
    d->sharedCtx.graphicsQueueIndex = d->queueIdx;

    d->platform = std::make_unique<AviQtlVulkanPlatform>();
    d->engine = filament::Engine::Builder().backend(filament::Engine::Backend::VULKAN).platform(d->platform.get()).sharedContext(static_cast<void *>(&d->sharedCtx)).build();

    if (!d->engine) {
        qCritical("[FilamentCanvas] filament::Engine::Builder::build() 失敗。");
        d->platform.reset();
        return;
    }

    d->renderer = d->engine->createRenderer();
    d->scene = d->engine->createScene();
    d->view = d->engine->createView();
    d->cameraEntity = utils::EntityManager::get().create();
    d->camera = d->engine->createCamera(d->cameraEntity);

    d->view->setScene(d->scene);
    d->view->setCamera(d->camera);
    d->view->setPostProcessingEnabled(false);

    // 仕様書準拠: 背景色 #001A33
    d->skybox = filament::Skybox::Builder().color({0.0f, 0.1f, 0.2f, 1.0f}).build(*d->engine);
    d->scene->setSkybox(d->skybox);

    qDebug("[FilamentCanvas] Filament Engine 初期化完了 (VulkanSharedContextQt)。");
}

// ─── Filament SwapChain から VkImage を取得 ────────────────────────────────

// Filament headless SwapChain の描画先 VkImage を取得する。
// AviQtlVulkanPlatform が保持する backend SwapChain handle を使う。
static VkImage getFilamentSwapChainImage(FilamentCanvasImpl *d) {
    if (!d->swapChain)
        return VK_NULL_HANDLE;

    auto *plat = d->platform.get();

    if (!plat) {
        qCritical("[FilamentCanvas] VulkanPlatform の取得に失敗しました。");
        return VK_NULL_HANDLE;
    }

    // public filament::SwapChain* と backend Platform::SwapChain* は別物。
    // MSVC では reinterpret_cast するとアクセス違反になるため、Platform が生成した
    // backend handle を記録して使う。
    auto *scPtr = plat->headlessSwapChain();
    if (!scPtr) {
        qCritical("[FilamentCanvas] Headless backend SwapChain の取得に失敗しました。");
        return VK_NULL_HANDLE;
    }
    filament::backend::VulkanPlatform::SwapChainBundle bundle = plat->getSwapChainBundle(scPtr);

    if (bundle.colors.empty()) {
        qCritical("[FilamentCanvas] SwapChainBundle.colors が空です。");
        return VK_NULL_HANDLE;
    }

    return bundle.colors[0];
}

// ─── オフスクリーン RenderTarget ─────────────────────────────────────────────

// プロジェクト解像度で Filament の RenderTarget を生成/再生成する。
// ウィンドウリサイズ時には呼ばれない。
static bool recreateOffscreenTarget(FilamentCanvasImpl *d) {
    const uint32_t w = d->renderW.load(std::memory_order_acquire);
    const uint32_t h = d->renderH.load(std::memory_order_acquire);

    if (!d->engineReady() || w == 0 || h == 0)
        return false;

    // 既にこの解像度で生成済みであれば何もしない
    if (d->swapChain && d->renderTarget && !d->swapChainDirty.load(std::memory_order_acquire))
        return true;

    // GPU 完了を待ってから既存リソースを安全に解放する
    if (d->renderTarget || d->swapChain)
        d->engine->flushAndWait();

    if (d->view)
        d->view->setRenderTarget(nullptr);
    if (d->renderTarget) {
        d->engine->destroy(d->renderTarget);
        d->renderTarget = nullptr;
    }
    if (d->colorTex) {
        d->engine->destroy(d->colorTex);
        d->colorTex = nullptr;
    }
    if (d->depthTex) {
        d->engine->destroy(d->depthTex);
        d->depthTex = nullptr;
    }
    // sgTexture は Filament SwapChain の VkImage を参照しているため先に解放する
    delete d->sgTexture;
    d->sgTexture = nullptr;
    d->lastBoundImage = VK_NULL_HANDLE;

    if (d->swapChain) {
        d->engine->destroy(d->swapChain);
        d->swapChain = nullptr;
    }

    qDebug("[FilamentCanvas] RenderTarget 生成: %u x %u", w, h);

    // headless SwapChain: CONFIG_READABLE で VkImage が getSwapChainBundle に公開される
    d->swapChain = d->engine->createSwapChain(w, h, filament::SwapChain::CONFIG_READABLE);
    if (!d->swapChain) {
        qCritical("[FilamentCanvas] createSwapChain(headless) 失敗。");
        return false;
    }
    d->engine->flushAndWait();

    d->colorTex = filament::Texture::Builder()
                      .width(w)
                      .height(h)
                      .levels(1)
                      .usage(filament::Texture::Usage::COLOR_ATTACHMENT | filament::Texture::Usage::SAMPLEABLE)
                      .format(filament::Texture::InternalFormat::RGBA8)
                      .build(*d->engine);

    d->depthTex = filament::Texture::Builder()
                      .width(w)
                      .height(h)
                      .levels(1)
                      .usage(filament::Texture::Usage::DEPTH_ATTACHMENT)
                      .format(filament::Texture::InternalFormat::DEPTH32F)
                      .build(*d->engine);

    if (!d->colorTex || !d->depthTex) {
        qCritical("[FilamentCanvas] Texture 生成失敗。");
        return false;
    }

    d->renderTarget = filament::RenderTarget::Builder()
                          .texture(filament::RenderTarget::AttachmentPoint::COLOR0, d->colorTex)
                          .texture(filament::RenderTarget::AttachmentPoint::DEPTH, d->depthTex)
                          .build(*d->engine);

    if (!d->renderTarget) {
        qCritical("[FilamentCanvas] RenderTarget 生成失敗。");
        return false;
    }

    d->view->setRenderTarget(d->renderTarget);
    d->view->setViewport({0, 0, w, h});

    // カメラの投影行列を初期化 (正投影)
    // 未初期化のカメラ（行列がすべて 0 や NaN）でレンダリングを実行すると、
    // Filament 内部のカリングやシェーダー計算で不正アクセスが発生しクラッシュする。
    if (d->camera) {
        d->camera->setProjection(filament::Camera::Projection::ORTHO, 0.0, (double)w, (double)h, 0.0, -1.0, 1.0);
    }

    d->swapChainDirty.store(false, std::memory_order_release);

    qDebug("[FilamentCanvas] RenderTarget 準備完了。");
    return true;
}

// ─── Filament 破棄 ────────────────────────────────────────────────────────────

static void destroyFilamentImpl(FilamentCanvasImpl *d) {
    if (!d->engineReady())
        return;

    // GPU 作業が完了するのを待つ (破棄前に必須)
    d->engine->flushAndWait();

    if (d->view)
        d->view->setRenderTarget(nullptr);
    if (d->renderTarget) {
        d->engine->destroy(d->renderTarget);
        d->renderTarget = nullptr;
    }
    if (d->colorTex) {
        d->engine->destroy(d->colorTex);
        d->colorTex = nullptr;
    }
    if (d->depthTex) {
        d->engine->destroy(d->depthTex);
        d->depthTex = nullptr;
    }

    // sgTexture は Filament VkImage を参照しているため先に解放する
    delete d->sgTexture;
    d->sgTexture = nullptr;
    d->lastBoundImage = VK_NULL_HANDLE;

    if (d->swapChain) {
        d->engine->destroy(d->swapChain);
        d->swapChain = nullptr;
    }

    if (d->camera) {
        d->engine->destroyCameraComponent(d->cameraEntity);
        utils::EntityManager::get().destroy(d->cameraEntity);
        d->camera = nullptr;
    }
    if (d->skybox) {
        d->engine->destroy(d->skybox);
        d->skybox = nullptr;
    }
    if (d->view) {
        d->engine->destroy(d->view);
        d->view = nullptr;
    }
    if (d->scene) {
        d->engine->destroy(d->scene);
        d->scene = nullptr;
    }
    if (d->renderer) {
        d->engine->destroy(d->renderer);
        d->renderer = nullptr;
    }

    filament::Engine::destroy(&d->engine);
    d->engine = nullptr;
    d->platform.reset();
    d->swapChainDirty.store(false, std::memory_order_release);

    qDebug("[FilamentCanvas] Filament Engine 破棄完了。");
}

// ─── レンダースレッドコールバック ─────────────────────────────────────────────

void FilamentCanvas::onBeforeRendering() {
    if (!m_window)
        return;

    auto *d = m_impl.get();

    if (!d->engineReady()) {
        initFilamentImpl(d, m_window);
        if (!d->engineReady())
            return;
    }

    // Filament はプロジェクト解像度で固定レンダリングする。
    // ウィンドウリサイズでは Qt SG が QSGSimpleTextureNode を scale する。
    if (!recreateOffscreenTarget(d))
        return;

    const bool frameBegun = d->renderer->beginFrame(d->swapChain);
    if (frameBegun) {
        d->renderer->render(d->view);
        d->renderer->endFrame();
        d->engine->flushAndWait();
    }

    m_frameDirty.store(true, std::memory_order_release);
    QMetaObject::invokeMethod(this, "update", Qt::QueuedConnection);
}

void FilamentCanvas::onSceneGraphInvalidated() { destroyFilamentImpl(m_impl.get()); }

// ─── Qt SceneGraph ノード ─────────────────────────────────────────────────────

QSGNode *FilamentCanvas::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *d = m_impl.get();

    if (!d->engineReady() || !m_window)
        return oldNode;

    if (!m_frameDirty.load(std::memory_order_acquire))
        return oldNode;
    m_frameDirty.store(false, std::memory_order_release);

    // Filament SwapChain の描画先 VkImage を取得する
    VkImage filamentImage = getFilamentSwapChainImage(d);
    if (filamentImage == VK_NULL_HANDLE)
        return oldNode;

    // SwapChain の VkImage が変わった場合 (解像度変更時のみ) は再生成する
    if (d->lastBoundImage != filamentImage) {
        delete d->sgTexture;
        d->sgTexture = nullptr;
        d->lastBoundImage = filamentImage;
    }

    if (!d->sgTexture) {
        // headless SwapChain の最終レイアウトは VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL。
        // Filament Vulkan バックエンドは headless の場合 COLOR_ATTACHMENT_OPTIMAL を使用する。
        const auto textureSize = QSize(static_cast<int>(d->renderW.load(std::memory_order_acquire)), static_cast<int>(d->renderH.load(std::memory_order_acquire)));
        d->sgTexture = QNativeInterface::QSGVulkanTexture::fromNative(filamentImage, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, m_window, textureSize);

        if (!d->sgTexture) {
            qWarning("[FilamentCanvas] QSGVulkanTexture::fromNative() 失敗。");
            return oldNode;
        }
    }

    auto *node = static_cast<QSGSimpleTextureNode *>(oldNode);
    if (!node) {
        node = new QSGSimpleTextureNode();
        node->setFiltering(QSGTexture::Linear);
    }

    node->setTexture(d->sgTexture);
    // ウィンドウサイズに合わせて scale して表示する
    // Filament は projectW x projectH で固定、Qt SG が引き伸ばす
    node->setRect(boundingRect());
    // Filament は Y 下向き、Qt SceneGraph は Y 上向き → 垂直反転
    node->setTextureCoordinatesTransform(QSGSimpleTextureNode::MirrorVertically);

    return node;
}

// ─── ジオメトリ変更 ───────────────────────────────────────────────────────────

void FilamentCanvas::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) {
        // Filament は固定解像度のため、ウィンドウリサイズでは
        // QSGSimpleTextureNode の rect を再設定するだけでよい。
        update();
    }
}

void FilamentCanvas::itemChange(ItemChange change, const ItemChangeData &value) { QQuickItem::itemChange(change, value); }

} // namespace AviQtl::Rendering
