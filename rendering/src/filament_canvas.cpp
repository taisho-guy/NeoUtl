// ─────────────────────────────────────────────────────────────────────────────
// filament_canvas.cpp  (フェーズ2 — VulkanSharedContextQt 実装)
//
// 参考実装:
//   - QML_Filament (OpenGL 版の基本設計パターン)
//   - mdk-qtquick-plugin (Qt6 Vulkan テクスチャ共有の実例)
//   - Qt 公式 Scene Graph - Vulkan Texture Import example
//
// アーキテクチャ:
//   1. Qt SceneGraph の VkDevice を QSGRendererInterface 経由で取得する
//      (QRhi/GuiPrivate を使わず、公開 API のみ使用)
//   2. 取得した VkDevice を Filament::Engine の sharedContext に渡し、
//      同一デバイス上で Filament を初期化する (VulkanSharedContext)
//   3. Filament はオフスクリーン RenderTarget に描画する
//   4. Qt SceneGraph は QNativeInterface::QSGVulkanTexture::fromNative() で
//      Filament の VkImage を QSGTexture としてラップして表示する (ゼロコピー)
//
// インクルード方針:
//   - VulkanPlatform.h は cpp 内にのみ登場する (hpp には露出させない)
//   - VK_NO_PROTOTYPES が定義されている場合は QVulkanDeviceFunctions を使う
// ─────────────────────────────────────────────────────────────────────────────

#include "filament_canvas.hpp"

// Qt 公開ヘッダ (MOC 安全)
#include <QDebug>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QSGSimpleTextureNode>
#include <QSGTexture>
#include <QVulkanFunctions>
#include <QVulkanInstance>

// QNativeInterface::QSGVulkanTexture は <QSGTexture> で定義されている
// (Qt6::Quick の公開 API)

// ─── Filament / Vulkan ヘッダ (cpp 内部のみ) ─────────────────────────────────
// VK_NO_PROTOTYPES が定義されていると vkXxx() のグローバルプロトタイプが無効化される。
// Filament の BlueVK がランタイムに関数ポインタを解決するため、
// 直接プロトタイプを呼び出す代わりに QVulkanDeviceFunctions を使う。
//
// ただし VulkanPlatform.h は bluevk/BlueVK.h を無条件でインクルードしており、
// これは Filament のビルドディレクトリにのみ存在する内部ヘッダである。
// 当 cpp ファイルでは VulkanPlatform.h を直接インクルードせず、
// Filament Engine の sharedContext 機構 (void* 経由) を使う設計を採用した。
// ─────────────────────────────────────────────────────────────────────────────

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

// VulkanSharedContext の定義を得るために VulkanPlatform.h を経由させる。
// ここでは cpp 内部のみのインクルードであるため bluevk の不在は問題にならない。
// ただし bluevk/BlueVK.h が見つからない場合は sharedContext を void* 経由で渡す。
// Engine::Builder::sharedContext(void*) は VulkanSharedContext* にキャストされる。
//
// Filament 側の VulkanSharedContext 構造体の定義:
//   struct VulkanSharedContext {
//       VkInstance instance;           // 外部 VkInstance
//       VkPhysicalDevice physicalDevice;
//       VkDevice logicalDevice;
//       uint32_t graphicsQueueFamilyIndex;
//       uint32_t graphicsQueueIndex;
//       bool debugUtilsSupported;
//       bool debugMarkersSupported;
//       bool multiviewSupported;
//   };
//
// これを手動で再定義して bluevk への依存を回避する。

// Vulkan 型の前方宣言 (VK_NO_PROTOTYPES でも有効)
#if defined(VK_NO_PROTOTYPES)
// VK_NO_PROTOTYPES 環境下では vulkan.h の型定義のみ使用する
// プロトタイプ (vkCreateImage 等) は QVulkanDeviceFunctions 経由で呼ぶ
#undef VK_NO_PROTOTYPES
#include <vulkan/vulkan.h>
#define VK_NO_PROTOTYPES 1
#else
#include <vulkan/vulkan.h>
#endif

namespace AviQtl::Rendering {

// ─────────────────────────────────────────────────────────────────────────────
// VulkanSharedContext (bluevk/BlueVK.h に依存しない手動定義)
//
// Filament の VulkanPlatform.h で定義されている構造体と ABI 互換である必要がある。
// Engine::Builder::sharedContext(void*) がこのポインタを VulkanSharedContext* に
// キャストして使う。フィールド順序と型を VulkanPlatform.h と完全に一致させる。
// ─────────────────────────────────────────────────────────────────────────────
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

// ─────────────────────────────────────────────────────────────────────────────
// FilamentCanvasImpl  (pimpl — Filament / Vulkan 依存をすべて閉じ込める)
// ─────────────────────────────────────────────────────────────────────────────
struct FilamentCanvasImpl {
    // Qt 側から受け取る Vulkan コンテキスト
    QVulkanInstance *qvkInstance = nullptr;
    VkInstance vkInstance = VK_NULL_HANDLE;
    VkPhysicalDevice physDev = VK_NULL_HANDLE;
    VkDevice dev = VK_NULL_HANDLE;
    uint32_t queueFamilyIdx = 0;

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

    // Qt SceneGraph 側の VkImage バッキング
    // QVulkanDeviceFunctions 経由で生成・破棄する
    VkImage colorImage = VK_NULL_HANDLE;
    VkDeviceMemory colorMemory = VK_NULL_HANDLE;

    // QSGTexture ラッパー (毎フレーム再生成しない)
    QSGTexture *sgTexture = nullptr;
    VkImage lastBoundImage = VK_NULL_HANDLE;

    uint32_t targetW = 0;
    uint32_t targetH = 0;

    bool engineReady() const noexcept { return engine != nullptr; }
};

// ─────────────────────────────────────────────────────────────────────────────
// FilamentCanvas — ctor / dtor
// ─────────────────────────────────────────────────────────────────────────────

FilamentCanvas::FilamentCanvas(QQuickItem *parent) : QQuickItem(parent), m_impl(std::make_unique<FilamentCanvasImpl>()) {
    setFlag(ItemHasContents, true);
    connect(this, &QQuickItem::windowChanged, this, &FilamentCanvas::handleWindowChanged);
}

FilamentCanvas::~FilamentCanvas() {
    if (m_window) {
        disconnect(m_beforeRenderingConn);
        disconnect(m_sceneGraphInvalidatedConn);
        disconnect(m_sgInitializedConn);
    }
    delete m_impl->sgTexture;
    m_impl->sgTexture = nullptr;
}

// ─── プロパティ ───────────────────────────────────────────────────────────────

int FilamentCanvas::sceneId() const noexcept { return m_sceneId; }
void FilamentCanvas::setSceneId(int id) {
    if (m_sceneId == id)
        return;
    m_sceneId = id;
    emit sceneIdChanged(id);
}

int FilamentCanvas::currentFrame() const noexcept { return m_currentFrame; }
void FilamentCanvas::setCurrentFrame(int frame) {
    if (m_currentFrame == frame)
        return;
    m_currentFrame = frame;
    emit currentFrameChanged(frame);
    m_frameDirty.store(true, std::memory_order_release);
    update();
}

// ─── ウィンドウ接続 ───────────────────────────────────────────────────────────

void FilamentCanvas::handleWindowChanged(QQuickWindow *win) {
    if (m_window) {
        disconnect(m_beforeRenderingConn);
        disconnect(m_sceneGraphInvalidatedConn);
        disconnect(m_sgInitializedConn);
    }
    m_window = win;
    if (!win)
        return;

    // レンダースレッドシグナルは DirectConnection で接続する
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

    // ── Qt SceneGraph から Vulkan リソースを取得 ──
    // QRhi / GuiPrivate に依存せず、QSGRendererInterface の公開 API のみ使用する。
    // (mdk-qtquick-plugin と Qt 公式 Vulkan texture import example 準拠)
    QSGRendererInterface *rif = win->rendererInterface();
    if (!rif) {
        qCritical("[FilamentCanvas] QSGRendererInterface が取得できません。");
        return;
    }

    if (rif->graphicsApi() != QSGRendererInterface::Vulkan) {
        qCritical("[FilamentCanvas] GraphicsApi が Vulkan ではありません。"
                  " QSG_RHI_BACKEND=vulkan 環境変数を確認してください。");
        return;
    }

    auto *qvkInst = reinterpret_cast<QVulkanInstance *>(rif->getResource(win, QSGRendererInterface::VulkanInstanceResource));
    auto physDev = *static_cast<VkPhysicalDevice *>(rif->getResource(win, QSGRendererInterface::PhysicalDeviceResource));
    auto dev = *static_cast<VkDevice *>(rif->getResource(win, QSGRendererInterface::DeviceResource));
    auto queueFamilyIdx = *static_cast<uint32_t *>(rif->getResource(win, QSGRendererInterface::GraphicsQueueFamilyIndexResource));

    if (!qvkInst || physDev == VK_NULL_HANDLE || dev == VK_NULL_HANDLE) {
        qCritical("[FilamentCanvas] Vulkan リソースが取得できません。"
                  " シーングラフが初期化されているか確認してください。");
        return;
    }

    d->qvkInstance = qvkInst;
    d->vkInstance = qvkInst->vkInstance();
    d->physDev = physDev;
    d->dev = dev;
    d->queueFamilyIdx = queueFamilyIdx;

    qDebug("[FilamentCanvas] Vulkan コンテキスト取得完了。Filament を初期化します。");

    // ── Filament Engine 初期化 ──
    // VulkanSharedContext (手動定義版) を void* にキャストして渡す。
    // Filament の Vulkan バックエンドはこれを VulkanSharedContext* にキャストして使う。
    FilamentVulkanSharedContext sharedCtx{};
    sharedCtx.instance = d->vkInstance;
    sharedCtx.physicalDevice = d->physDev;
    sharedCtx.logicalDevice = d->dev;
    sharedCtx.graphicsQueueFamilyIndex = d->queueFamilyIdx;
    sharedCtx.graphicsQueueIndex = 0;

    d->engine = filament::Engine::Builder().backend(filament::Engine::Backend::VULKAN).sharedContext(static_cast<void *>(&sharedCtx)).build();

    if (!d->engine) {
        qCritical("[FilamentCanvas] filament::Engine::Builder::build() 失敗。");
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

    // 仕様書準拠: 背景色 #001A33 (紺色)
    d->skybox = filament::Skybox::Builder().color({0.0f, 0.1f, 0.2f, 1.0f}).build(*d->engine);
    d->scene->setSkybox(d->skybox);

    qDebug("[FilamentCanvas] Filament Engine 初期化完了 (VulkanSharedContextQt)。");
}

// ─── VkImage の確保 / 解放 (QVulkanDeviceFunctions 経由) ─────────────────────

static bool allocVkImage(FilamentCanvasImpl *d, uint32_t w, uint32_t h) {
    QVulkanDeviceFunctions *df = d->qvkInstance->deviceFunctions(d->dev);

    VkImageCreateInfo imgInfo{};
    imgInfo.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO;
    imgInfo.imageType = VK_IMAGE_TYPE_2D;
    imgInfo.format = VK_FORMAT_R8G8B8A8_UNORM;
    imgInfo.extent = {w, h, 1};
    imgInfo.mipLevels = 1;
    imgInfo.arrayLayers = 1;
    imgInfo.samples = VK_SAMPLE_COUNT_1_BIT;
    imgInfo.tiling = VK_IMAGE_TILING_OPTIMAL;
    imgInfo.usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT;
    imgInfo.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;

    if (df->vkCreateImage(d->dev, &imgInfo, nullptr, &d->colorImage) != VK_SUCCESS) {
        qCritical("[FilamentCanvas] vkCreateImage 失敗。");
        return false;
    }

    VkMemoryRequirements memReq{};
    df->vkGetImageMemoryRequirements(d->dev, d->colorImage, &memReq);

    // physDev 側の関数は QVulkanFunctions で取得する
    QVulkanFunctions *f = d->qvkInstance->functions();
    VkPhysicalDeviceMemoryProperties memProps{};
    f->vkGetPhysicalDeviceMemoryProperties(d->physDev, &memProps);

    uint32_t memTypeIdx = UINT32_MAX;
    for (uint32_t i = 0; i < memProps.memoryTypeCount; ++i) {
        if ((memReq.memoryTypeBits & (1u << i)) && (memProps.memoryTypes[i].propertyFlags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)) {
            memTypeIdx = i;
            break;
        }
    }
    if (memTypeIdx == UINT32_MAX) {
        qCritical("[FilamentCanvas] 適切なメモリタイプが見つかりません。");
        df->vkDestroyImage(d->dev, d->colorImage, nullptr);
        d->colorImage = VK_NULL_HANDLE;
        return false;
    }

    VkMemoryAllocateInfo allocInfo{};
    allocInfo.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    allocInfo.allocationSize = memReq.size;
    allocInfo.memoryTypeIndex = memTypeIdx;

    if (df->vkAllocateMemory(d->dev, &allocInfo, nullptr, &d->colorMemory) != VK_SUCCESS) {
        qCritical("[FilamentCanvas] vkAllocateMemory 失敗。");
        df->vkDestroyImage(d->dev, d->colorImage, nullptr);
        d->colorImage = VK_NULL_HANDLE;
        return false;
    }

    df->vkBindImageMemory(d->dev, d->colorImage, d->colorMemory, 0);
    return true;
}

static void freeVkImage(FilamentCanvasImpl *d) {
    if (!d->qvkInstance || d->dev == VK_NULL_HANDLE)
        return;
    QVulkanDeviceFunctions *df = d->qvkInstance->deviceFunctions(d->dev);
    if (d->colorImage != VK_NULL_HANDLE) {
        df->vkDestroyImage(d->dev, d->colorImage, nullptr);
        d->colorImage = VK_NULL_HANDLE;
    }
    if (d->colorMemory != VK_NULL_HANDLE) {
        df->vkFreeMemory(d->dev, d->colorMemory, nullptr);
        d->colorMemory = VK_NULL_HANDLE;
    }
}

// ─── オフスクリーン RenderTarget ─────────────────────────────────────────────

static bool recreateOffscreenTarget(FilamentCanvasImpl *d, uint32_t w, uint32_t h) {
    if (!d->engineReady() || w == 0 || h == 0)
        return false;
    if (d->renderTarget && d->targetW == w && d->targetH == h)
        return true;

    // 既存リソースを解放する
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
    if (d->swapChain) {
        d->engine->destroy(d->swapChain);
        d->swapChain = nullptr;
    }
    freeVkImage(d);

    delete d->sgTexture;
    d->sgTexture = nullptr;
    d->lastBoundImage = VK_NULL_HANDLE;

    qDebug("[FilamentCanvas] RenderTarget 生成: %u x %u", w, h);

    // VkImage を確保する
    if (!allocVkImage(d, w, h))
        return false;

    // Filament ヘッドレス SwapChain (幅 × 高さ指定)
    d->swapChain = d->engine->createSwapChain(w, h, filament::SwapChain::CONFIG_READABLE);
    if (!d->swapChain) {
        qCritical("[FilamentCanvas] createSwapChain(headless) 失敗。");
        freeVkImage(d);
        return false;
    }

    d->colorTex = filament::Texture::Builder().width(w).height(h).levels(1).usage(filament::Texture::Usage::COLOR_ATTACHMENT | filament::Texture::Usage::SAMPLEABLE).format(filament::Texture::InternalFormat::RGBA8).build(*d->engine);

    d->depthTex = filament::Texture::Builder().width(w).height(h).levels(1).usage(filament::Texture::Usage::DEPTH_ATTACHMENT).format(filament::Texture::InternalFormat::DEPTH32F).build(*d->engine);

    if (!d->colorTex || !d->depthTex) {
        qCritical("[FilamentCanvas] Texture 生成失敗。");
        return false;
    }

    d->renderTarget = filament::RenderTarget::Builder().texture(filament::RenderTarget::AttachmentPoint::COLOR0, d->colorTex).texture(filament::RenderTarget::AttachmentPoint::DEPTH, d->depthTex).build(*d->engine);

    if (!d->renderTarget) {
        qCritical("[FilamentCanvas] RenderTarget 生成失敗。");
        return false;
    }

    d->view->setRenderTarget(d->renderTarget);
    d->view->setViewport({0, 0, w, h});
    d->targetW = w;
    d->targetH = h;

    qDebug("[FilamentCanvas] RenderTarget 準備完了。");
    return true;
}

// ─── Filament 破棄 ────────────────────────────────────────────────────────────

static void destroyFilamentImpl(FilamentCanvasImpl *d) {
    if (!d->engineReady())
        return;

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
    if (d->swapChain) {
        d->engine->destroy(d->swapChain);
        d->swapChain = nullptr;
    }

    freeVkImage(d);

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

    delete d->sgTexture;
    d->sgTexture = nullptr;
    d->lastBoundImage = VK_NULL_HANDLE;
    d->targetW = d->targetH = 0;

    qDebug("[FilamentCanvas] Filament Engine 破棄完了。");
}

// ─── レンダースレッドコールバック ─────────────────────────────────────────────

void FilamentCanvas::onBeforeRendering() {
    if (!m_window)
        return;

    const double dpr = m_window->devicePixelRatio();
    const uint32_t pw = static_cast<uint32_t>(width() * dpr);
    const uint32_t ph = static_cast<uint32_t>(height() * dpr);
    if (pw == 0 || ph == 0)
        return;

    auto *d = m_impl.get();

    if (!d->engineReady()) {
        initFilamentImpl(d, m_window);
        if (!d->engineReady())
            return;
    }

    if (!recreateOffscreenTarget(d, pw, ph))
        return;

    // Filament に描画させる
    if (d->renderer->beginFrame(d->swapChain)) {
        d->renderer->render(d->view);
        d->renderer->endFrame();
    }

    m_targetW = pw;
    m_targetH = ph;
    m_frameDirty.store(true, std::memory_order_release);
    QMetaObject::invokeMethod(this, "update", Qt::QueuedConnection);
}

void FilamentCanvas::onSceneGraphInvalidated() { destroyFilamentImpl(m_impl.get()); }

// ─── Qt SceneGraph ノード ─────────────────────────────────────────────────────

QSGNode *FilamentCanvas::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *d = m_impl.get();

    if (!d->engineReady() || !m_window || d->colorImage == VK_NULL_HANDLE) {
        return oldNode;
    }

    if (!m_frameDirty.load(std::memory_order_acquire)) {
        return oldNode;
    }
    m_frameDirty.store(false, std::memory_order_release);

    // サイズ変更で VkImage が変わった場合は QSGTexture を再生成する
    if (d->lastBoundImage != d->colorImage) {
        delete d->sgTexture;
        d->sgTexture = nullptr;
        d->lastBoundImage = d->colorImage;
    }

    // Qt 6.0+ 公式 API: QNativeInterface::QSGVulkanTexture::fromNative()
    // VkImage を QSGTexture にゼロコピーでラップする。
    // (#include <QSGTexture> のみ必要。priv ヘッダ不要)
    if (!d->sgTexture) {
        d->sgTexture = QNativeInterface::QSGVulkanTexture::fromNative(d->colorImage, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, m_window, QSize(static_cast<int>(m_targetW), static_cast<int>(m_targetH)));

        if (!d->sgTexture) {
            qWarning("[FilamentCanvas] QSGVulkanTexture::fromNative() 失敗。"
                     " シーングラフが初期化されているか確認してください。");
            return oldNode;
        }
    }

    auto *node = static_cast<QSGSimpleTextureNode *>(oldNode);
    if (!node) {
        node = new QSGSimpleTextureNode();
        node->setFiltering(QSGTexture::Linear);
    }

    node->setTexture(d->sgTexture);
    node->setRect(boundingRect());
    // Filament は Y 下向き、Qt SceneGraph は Y 上向き → 垂直反転で補正
    node->setTextureCoordinatesTransform(QSGSimpleTextureNode::MirrorVertically);

    return node;
}

// ─── ジオメトリ変更 ───────────────────────────────────────────────────────────

void FilamentCanvas::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) {
        // サイズが変わったら RenderTarget を再生成する (次フレームで実行)
        m_targetW = m_targetH = 0;
        auto *d = m_impl.get();
        delete d->sgTexture;
        d->sgTexture = nullptr;
        d->lastBoundImage = VK_NULL_HANDLE;
        update();
    }
}

void FilamentCanvas::itemChange(ItemChange change, const ItemChangeData &value) { QQuickItem::itemChange(change, value); }

} // namespace AviQtl::Rendering
