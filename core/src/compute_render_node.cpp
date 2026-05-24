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

void ComputeRenderNode::syncSize(float w, float h) {
    if (qFuzzyCompare(m_width, w) && qFuzzyCompare(m_height, h))
        return;
    m_width = w;
    m_height = h;
    m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncInputTexture(QSGTexture *tex) {
    if (m_inputTexture == tex)
        return;
    m_inputTexture = tex;
    m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncWorkGroupSize(int x, int y, int z) {
    m_workGroupX = qMax(1, x);
    m_workGroupY = qMax(1, y);
    m_workGroupZ = qMax(1, z);
}

QRectF ComputeRenderNode::rect() const { return QRectF(0, 0, m_width, m_height); }

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
    if (!rhi->isFeatureSupported(QRhi::Compute)) {
        qCWarning(lcComputeRenderNode) << "Compute shaders are not supported on this hardware/backend.";
        return false;
    }

    bool textureSizeChanged = false;
    if (m_inputTexture) {
        QSize sz = m_inputTexture->textureSize();
        if (!m_outputTexture || m_outputTexture->pixelSize() != sz)
            textureSizeChanged = true;
    }

    bool needsRebuild = m_bufferLayoutDirty || textureSizeChanged;
    if (!needsRebuild && m_gpuBuffers.size() == m_pendingSSBOs.size()) {
        for (int i = 0; i < m_pendingSSBOs.size(); ++i) {
            if (m_gpuBuffers[i].size < static_cast<quint32>(m_pendingSSBOs[i].data.size())) {
                needsRebuild = true;
                break;
            }
        }
    }

    if (!needsRebuild)
        // バッファがなくてもテクスチャ（画像処理）があれば有効とみなす
        return (m_inputTexture != nullptr) || !m_gpuBuffers.isEmpty();

    // 既存 GPU バッファと SRB を安全に破棄
    for (auto &gb : m_gpuBuffers) {
        delete gb.buf;
        gb.buf = nullptr;
    }
    m_gpuBuffers.clear();
    if (m_outputTexture) {
        delete m_outputTexture;
        m_outputTexture = nullptr;
    }
    if (m_sampler) {
        delete m_sampler;
        m_sampler = nullptr;
    }
    if (m_vbuf) {
        delete m_vbuf;
        m_vbuf = nullptr;
    }
    if (m_ubuf) {
        delete m_ubuf;
        m_ubuf = nullptr;
    }
    if (m_renderSrb) {
        delete m_renderSrb;
        m_renderSrb = nullptr;
    }
    if (m_renderPipeline) {
        delete m_renderPipeline;
        m_renderPipeline = nullptr;
    }
    delete m_srb;
    m_srb = nullptr;

    if (m_pendingSSBOs.isEmpty()) {
        // バッファが空の場合はレイアウトが確定したとみなす
        if (m_gpuBuffers.isEmpty()) {
            m_bufferLayoutDirty = false;
        }
        if (!m_inputTexture)
            return false;
    }

    m_srb = rhi->newShaderResourceBindings();
    QList<QRhiShaderResourceBinding> bindings;

    // Binding 0 & 1: 画像処理用の入出力テクスチャ
    if (m_inputTexture) {
        QRhiTexture *inRhiTex = m_inputTexture->rhiTexture();
        if (inRhiTex) {
            if (!m_sampler) {
                m_sampler = rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear, QRhiSampler::None, QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge);
                m_sampler->create();
            }
            // 入力サンプラー (Binding 0)
            bindings.append(QRhiShaderResourceBinding::sampledTexture(0, QRhiShaderResourceBinding::ComputeStage, inRhiTex, m_sampler));

            // 出力イメージ (Binding 1)
            QSize sz = m_inputTexture->textureSize();
            m_outputTexture = rhi->newTexture(QRhiTexture::RGBA8, sz, 1, QRhiTexture::UsedWithLoadStore | QRhiTexture::RenderTarget);
            if (!m_outputTexture->create()) {
                return false;
            }
            bindings.append(QRhiShaderResourceBinding::imageLoadStore(1, QRhiShaderResourceBinding::ComputeStage, m_outputTexture, 0));

            // 描画用リソースの初期化 (Full-screen quad)
            static const float quadData[] = {0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 1.0f, 0.0f, 1.0f, 1.0f, 0.0f, 1.0f, 0.0f, 1.0f, 1.0f, 1.0f, 1.0f};
            m_vbuf = rhi->newBuffer(QRhiBuffer::Immutable, QRhiBuffer::VertexBuffer, sizeof(quadData));
            m_vbuf->create();

            QRhiResourceUpdateBatch *batch = rhi->nextResourceUpdateBatch();
            batch->uploadStaticBuffer(m_vbuf, quadData);

            m_ubuf = rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 64); // Matrix用
            m_ubuf->create();

            m_renderSrb = rhi->newShaderResourceBindings();
            m_renderSrb->setBindings({QRhiShaderResourceBinding::uniformBuffer(0, QRhiShaderResourceBinding::VertexStage, m_ubuf), QRhiShaderResourceBinding::sampledTexture(1, QRhiShaderResourceBinding::FragmentStage, m_outputTexture, m_sampler)});
            m_renderSrb->create();
        }
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
        // SSBO は Binding 2 以降へオフセットしてバインド
        bindings.append(QRhiShaderResourceBinding::bufferLoad(entry.binding + 2, QRhiShaderResourceBinding::ComputeStage, buf));
    }

    m_srb->setBindings(bindings.cbegin(), bindings.cend());
    if (!m_srb->create()) {
        qCWarning(lcComputeRenderNode) << "QRhiShaderResourceBindings::create() 失敗";
        return false;
    }

    m_bufferLayoutDirty = false;
    m_shaderDirty = true;
    return true;
}

bool ComputeRenderNode::ensurePipeline(QRhi *rhi) {
    if (!m_shaderDirty && m_pipeline && m_renderPipeline)
        return true;

    if (m_pipeline)
        delete m_pipeline;
    m_pipeline = nullptr;

    if (m_renderPipeline)
        delete m_renderPipeline;
    m_renderPipeline = nullptr;

    auto *ri = m_window->rendererInterface();

    // 1. 表示用グラフィックスパイプラインの構築
    if (m_renderSrb) {
        m_renderPipeline = rhi->newGraphicsPipeline();
        m_renderPipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
        m_renderPipeline->setShaderResourceBindings(m_renderSrb);

        QRhiVertexInputLayout inputLayout;
        inputLayout.setBindings({{4 * sizeof(float)}});
        inputLayout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0}, {0, 1, QRhiVertexInputAttribute::Float2, 2 * sizeof(float)}});
        m_renderPipeline->setVertexInputLayout(inputLayout);

        QFile vfile(QStringLiteral(":/qt/qml/AviQtl/ui/qml/common/shaders/blit.vert.qsb"));
        QFile ffile(QStringLiteral(":/qt/qml/AviQtl/ui/qml/common/shaders/blit.frag.qsb"));
        if (vfile.open(QIODevice::ReadOnly) && ffile.open(QIODevice::ReadOnly)) {
            m_renderPipeline->setShaderStages({{QRhiShaderStage::Vertex, QShader::fromSerialized(vfile.readAll())}, {QRhiShaderStage::Fragment, QShader::fromSerialized(ffile.readAll())}});
        }
    }

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

    if (m_renderPipeline) {
        auto *rt = static_cast<QRhiRenderPassDescriptor *>(ri->getResource(m_window, QSGRendererInterface::RenderPassResource));
        m_renderPipeline->setTargetBlends({{}});
        m_renderPipeline->setRenderPassDescriptor(rt);
        m_renderPipeline->create();
    }

    m_shaderDirty = false;
    qCDebug(lcComputeRenderNode) << "Compute/Graphics パイプライン構築完了:" << m_shaderPath;
    return true;
}

void ComputeRenderNode::prepare() {
    m_rhi = resolveRhi();
    if (!m_rhi)
        return;

    if (!ensureBuffers(m_rhi))
        return;
    if (!ensurePipeline(m_rhi))
        return;

    // 実行リソースが揃っているなら、フラグに関わらず実行を許可
    // (テクスチャの内容そのものが変わっている可能性があるため)
    if (!m_pipeline || !m_srb)
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

void ComputeRenderNode::render(const RenderState *state) {
    auto *cb = resolveCommandBuffer();
    if (!cb || !m_renderPipeline || !m_vbuf || !m_outputTexture)
        return;

    // 行列の更新 (QMLアイテムの論理座標からNDCへの変換)
    QRhiResourceUpdateBatch *batch = m_rhi->nextResourceUpdateBatch();
    // projectionMatrix() は Item 空間から NDC への変換を保持しているため、デリファレンスして取得
    // 頂点データが 0..1 の単位正方形のため、アイテムの論理サイズまでスケールをかける
    QMatrix4x4 mat = (state && state->projectionMatrix()) ? *state->projectionMatrix() : QMatrix4x4();
    mat.scale(m_width, m_height, 1.0f);
    batch->updateDynamicBuffer(m_ubuf, 0, 64, mat.constData());

    cb->resourceUpdate(batch);

    cb->setGraphicsPipeline(m_renderPipeline);

    // Qt 6.11 では RenderState から直接矩形が得られないため、ウィンドウの物理サイズをビューポートに使用
    const float dpr = m_window->devicePixelRatio();
    cb->setViewport(QRhiViewport(0, 0, static_cast<float>(m_window->width()) * dpr, static_cast<float>(m_window->height()) * dpr));

    // シーングラフによるクリッピングが有効な場合は適用
    if (state && state->scissorEnabled()) {
        const QRect s = state->scissorRect();
        cb->setScissor(QRhiScissor(s.x(), s.y(), s.width(), s.height()));
    }

    cb->setShaderResources(m_renderSrb);

    const QRhiCommandBuffer::VertexInput vbufBinding(m_vbuf, 0);
    cb->setVertexInput(0, 1, &vbufBinding);
    cb->draw(4);
}

void ComputeRenderNode::releaseResources() { destroyResources(); }

void ComputeRenderNode::destroyResources() {
    delete m_pipeline;
    m_pipeline = nullptr;
    delete m_srb;
    m_srb = nullptr;
    delete m_renderPipeline;
    m_renderPipeline = nullptr;
    delete m_renderSrb;
    m_renderSrb = nullptr;
    delete m_outputTexture;
    m_outputTexture = nullptr;
    delete m_sampler;
    m_sampler = nullptr;
    delete m_vbuf;
    m_vbuf = nullptr;
    delete m_ubuf;
    m_ubuf = nullptr;

    for (auto &gb : m_gpuBuffers) {
        if (gb.buf)
            delete gb.buf;
        gb.buf = nullptr;
    }
    m_gpuBuffers.clear();
    m_bufferLayoutDirty = true;
    m_shaderDirty = true;
}

QSGRenderNode::StateFlags ComputeRenderNode::changedStates() const { return {}; }

QSGRenderNode::RenderingFlags ComputeRenderNode::flags() const {
    // BoundedRectRendering: バウンディングボックス内のみ描画
    // OpaqueRendering: アルファブレンド不要
    return BoundedRectRendering | OpaqueRendering;
}

} // namespace AviQtl::UI::Effects
