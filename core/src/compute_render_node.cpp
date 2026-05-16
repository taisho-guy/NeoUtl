#include "compute_render_node.hpp"
#include <QFile>
#include <QLoggingCategory>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <rhi/qrhi.h>

Q_LOGGING_CATEGORY(lcComputeRenderNode, "aviqtl.compute_render_node")

namespace AviQtl::UI::Effects {

ComputeRenderNode::ComputeRenderNode(QQuickWindow *window) : m_window(window) {}

ComputeRenderNode::~ComputeRenderNode() { destroyResources(); }

void ComputeRenderNode::syncSSBOs(const QList<SSBOEntry> &entries) {
    // バッファの binding セットが変わった場合はレイアウト再構築フラグを立てる
    bool layoutChanged = (m_gpuBuffers.size() != entries.size());
    if (!layoutChanged) {
        for (int i = 0; i < entries.size(); ++i) {
            if (m_gpuBuffers[i].binding != entries[i].binding) {
                layoutChanged = true;
                break;
            }
        }
    }
    m_pendingSSBOs = entries;
    m_ssboDirty = !entries.isEmpty();
    if (layoutChanged)
        m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncShaderPath(const QString &path) {
    if (m_shaderPath == path)
        return;
    m_shaderPath = path;
    m_shaderDirty = true;
}

void ComputeRenderNode::syncWorkGroupSize(int x, int y, int z) {
    m_workGroupX = qMax(1, x);
    m_workGroupY = qMax(1, y);
    m_workGroupZ = qMax(1, z);
}

QRhi *ComputeRenderNode::resolveRhi() const { return static_cast<QRhi *>(m_window->rendererInterface()->getResource(m_window, QSGRendererInterface::RhiResource)); }

QRhiCommandBuffer *ComputeRenderNode::resolveCommandBuffer() const {
    auto *ri = m_window->rendererInterface();
    // Qt 6.6+: RhiRedirectCommandBuffer がメインレンダーパス前のコマンドバッファ
    // それ以前: CommandListResource にフォールバック
    auto *cb = static_cast<QRhiCommandBuffer *>(ri->getResource(m_window, QSGRendererInterface::RhiRedirectCommandBuffer));
    if (!cb) {
        cb = static_cast<QRhiCommandBuffer *>(ri->getResource(m_window, QSGRendererInterface::CommandListResource));
    }
    return cb;
}

bool ComputeRenderNode::ensureBuffers(QRhi *rhi) {
    // 1. レイアウト変更 (個数/バインディング) または バッファサイズ増大のチェック
    bool needsRebuild = m_bufferLayoutDirty;
    if (!needsRebuild && m_gpuBuffers.size() == m_pendingSSBOs.size()) {
        for (int i = 0; i < m_pendingSSBOs.size(); ++i) {
            if (m_gpuBuffers[i].size < static_cast<quint32>(m_pendingSSBOs[i].data.size())) {
                needsRebuild = true;
                break;
            }
        }
    }

    if (!needsRebuild)
        return !m_gpuBuffers.isEmpty();

    // 既存 GPU バッファと SRB を安全に破棄
    for (auto &gb : m_gpuBuffers) {
        delete gb.buf;
        gb.buf = nullptr;
    }
    m_gpuBuffers.clear();
    delete m_srb;
    m_srb = nullptr;

    if (m_pendingSSBOs.isEmpty()) {
        m_bufferLayoutDirty = false;
        return false;
    }

    // 各 SSBO エントリに対応するバッファを確保
    for (const auto &entry : std::as_const(m_pendingSSBOs)) {
        const qsizetype sz = entry.data.size();
        if (sz == 0)
            continue;
        // 頻繁な再確保を避けるため、少し余裕を持って確保
        auto *buf = rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::StorageBuffer, static_cast<quint32>(sz));
        if (!buf->create()) {
            qCWarning(lcComputeRenderNode) << "QRhiBuffer::create() 失敗 binding=" << entry.binding;
            delete buf;
            return false;
        }
        m_gpuBuffers.push_back({entry.binding, buf, static_cast<quint32>(sz)});
    }

    // SRB 構築: GpuClipSoA は Compute Shader からの読み取り専用なので bufferLoad を使用
    m_srb = rhi->newShaderResourceBindings();
    QList<QRhiShaderResourceBinding> bindings;
    bindings.reserve(m_gpuBuffers.size());
    for (const auto &gb : std::as_const(m_gpuBuffers)) {
        bindings.append(QRhiShaderResourceBinding::bufferLoad(gb.binding, QRhiShaderResourceBinding::ComputeStage, gb.buf));
    }
    m_srb->setBindings(bindings.cbegin(), bindings.cend());
    if (!m_srb->create()) {
        qCWarning(lcComputeRenderNode) << "QRhiShaderResourceBindings::create() 失敗";
        return false;
    }

    m_bufferLayoutDirty = false;
    // バッファレイアウト変更 → パイプラインも SRB を参照しているため再構築が必要
    m_shaderDirty = true;
    return true;
}

bool ComputeRenderNode::ensurePipeline(QRhi *rhi) {
    if (!m_shaderDirty)
        return m_pipeline != nullptr;

    if (m_pipeline)
        delete m_pipeline;
    m_pipeline = nullptr;

    if (m_shaderPath.isEmpty()) {
        qCDebug(lcComputeRenderNode) << "shaderPath 未設定 (パイプライン構築スキップ)";
        return false;
    }
    if (!m_srb) {
        qCWarning(lcComputeRenderNode) << "SRB が未構築: ensureBuffers() を先に呼ぶこと";
        return false;
    }

    // .qsb ファイルをロード (Qt Shader Baker 出力形式)
    QFile f(m_shaderPath);
    if (!f.open(QIODevice::ReadOnly)) {
        qCWarning(lcComputeRenderNode) << "シェーダーファイルが開けません:" << m_shaderPath;
        return false;
    }
    m_shader = QShader::fromSerialized(f.readAll());
    if (!m_shader.isValid()) {
        qCWarning(lcComputeRenderNode) << "無効な .qsb ファイル:" << m_shaderPath;
        return false;
    }

    m_pipeline = rhi->newComputePipeline();
    m_pipeline->setShaderStage({QRhiShaderStage::Compute, m_shader});
    m_pipeline->setShaderResourceBindings(m_srb);
    if (!m_pipeline->create()) {
        qCWarning(lcComputeRenderNode) << "QRhiComputePipeline::create() 失敗";
        delete m_pipeline;
        m_pipeline = nullptr;
        return false;
    }

    m_shaderDirty = false;
    qCDebug(lcComputeRenderNode) << "パイプライン構築完了:" << m_shaderPath;
    return true;
}

void ComputeRenderNode::prepare() {
    m_rhi = resolveRhi();
    if (!m_rhi)
        return;

    // バッファ確保 → SRB 構築
    if (!ensureBuffers(m_rhi))
        return;

    // パイプライン構築 (シェーダーロード + QRhiComputePipeline::create())
    if (!ensurePipeline(m_rhi))
        return;

    if (!m_ssboDirty)
        return;

    auto *cb = resolveCommandBuffer();
    if (!cb)
        return;

    // CPU → GPU アップロードバッチを構築
    QRhiResourceUpdateBatch *batch = m_rhi->nextResourceUpdateBatch();
    for (const auto &entry : std::as_const(m_pendingSSBOs)) {
        for (const auto &gb : std::as_const(m_gpuBuffers)) {
            if (gb.binding != entry.binding || !gb.buf)
                continue;
            // Dynamic バッファの部分更新: サイズ不一致は ensureBuffers 内の再確保で吸収済み
            const quint32 uploadSize = static_cast<quint32>(qMin(entry.data.size(), gb.size));
            batch->updateDynamicBuffer(gb.buf, 0, uploadSize, entry.data.constData());
            break;
        }
    }

    // Compute パスを実行 (batch は beginComputePass で消費される)
    cb->beginComputePass(batch);
    cb->setComputePipeline(m_pipeline);
    cb->setShaderResources(m_srb);
    cb->dispatch(m_workGroupX, m_workGroupY, m_workGroupZ);
    cb->endComputePass();

    m_ssboDirty = false;
}

void ComputeRenderNode::render(const RenderState *) {
    // フェーズ8: 実 GPU 処理は prepare() の Compute Dispatch で完結
    // フェーズ9 でここに SceneGraph Compositor の出力テクスチャブリットを追加する
}

void ComputeRenderNode::releaseResources() { destroyResources(); }

void ComputeRenderNode::destroyResources() {
    if (m_pipeline)
        delete m_pipeline;
    m_pipeline = nullptr;
    if (m_srb)
        delete m_srb;
    m_srb = nullptr;
    for (auto &gb : m_gpuBuffers) {
        if (gb.buf)
            delete gb.buf;
        gb.buf = nullptr;
    }
    m_gpuBuffers.clear();
    m_bufferLayoutDirty = true;
    m_shaderDirty = true;
}

QSGRenderNode::StateFlags ComputeRenderNode::changedStates() const {
    // Compute パスはグラフィクスレンダーステートを変更しない
    return {};
}

QSGRenderNode::RenderingFlags ComputeRenderNode::flags() const {
    // BoundedRectRendering: バウンディングボックス内のみ描画
    // OpaqueRendering: アルファブレンド不要
    return BoundedRectRendering | OpaqueRendering;
}

} // namespace AviQtl::UI::Effects
