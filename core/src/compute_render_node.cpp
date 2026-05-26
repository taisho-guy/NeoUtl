#include "compute_render_node.hpp"
#include <QColor>
#include <QCoreApplication>
#include <QFile>
#include <QLoggingCategory>
#include <QPointF>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <cstring>
#include <rhi/qrhi.h>
#include <rhi/qshaderdescription.h>

Q_LOGGING_CATEGORY(lcComputeRenderNode, "aviqtl.compute_render_node")

namespace AviQtl::UI::Effects {

namespace {
static constexpr int kOutputBinding = 0;
static constexpr int kInputBinding = 1;
static constexpr int kParamsBinding = 2;
static constexpr int kParamsBlockSize = 32;

static const float kQuadData[] = {
    0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 1.0f, 0.0f, 1.0f, 1.0f, 0.0f, 1.0f, 0.0f, 1.0f, 1.0f, 1.0f, 1.0f,
};
} // namespace

ComputeRenderNode::ComputeRenderNode(QQuickWindow *window) : m_window(window) {}

ComputeRenderNode::~ComputeRenderNode() { destroyResources(); }

void ComputeRenderNode::syncParams(const QVariantMap &params) {
    if (m_params == params)
        return;
    m_params = params;
    m_paramsDirty = true;
    m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncShaderPath(const QString &path) {
    if (m_shaderPath == path)
        return;
    m_shaderPath = path;
    m_shaderDirty = true;
    m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncSize(float w, float h) {
    if (qFuzzyCompare(m_width, w) && qFuzzyCompare(m_height, h))
        return;
    qCDebug(lcComputeRenderNode) << "ComputeRenderNode: Size changed to" << w << "x" << h;
    m_width = w;
    m_height = h;
    m_bufferLayoutDirty = true;
}

void ComputeRenderNode::syncInputTexture(QSGTexture *tex) {
    if (m_inputTexture == tex)
        return;
    qCDebug(lcComputeRenderNode) << "ComputeRenderNode: inputTexture updated to" << tex;
    m_inputTexture = tex;
    m_bufferLayoutDirty = true;
    m_renderTargetDirty = true;
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
    auto *cb = static_cast<QRhiCommandBuffer *>(ri->getResource(m_window, QSGRendererInterface::RhiRedirectCommandBuffer));
    if (!cb) {
        cb = static_cast<QRhiCommandBuffer *>(ri->getResource(m_window, QSGRendererInterface::CommandListResource));
    }
    return cb;
}

bool ComputeRenderNode::ensureBuffers(QRhi *rhi) {
    const bool computeSupported = rhi->isFeatureSupported(QRhi::Compute);
    QRhiTexture *currentInputRhiTexture = m_inputTexture ? m_inputTexture->rhiTexture() : nullptr;
    if (m_inputRhiTexture != currentInputRhiTexture) {
        m_inputRhiTexture = currentInputRhiTexture;
        m_bufferLayoutDirty = true;
        m_renderTargetDirty = true;
    }

    if (!m_shaderPath.isEmpty() && m_shaderDirty) {
        QFile f(m_shaderPath);
        if (f.open(QIODevice::ReadOnly)) {
            QShader nextShader = QShader::fromSerialized(f.readAll());
            if (nextShader.isValid()) {
                m_shader = nextShader;
                m_shaderDirty = false;
            } else {
                m_error = QStringLiteral("Compute shader file is invalid: %1").arg(m_shaderPath);
            }
        } else {
            m_error = QStringLiteral("Compute shader file cannot be opened: %1").arg(m_shaderPath);
        }
    }

    bool textureSizeChanged = false;
    if (computeSupported && m_inputTexture) {
        QSize sz = m_inputTexture->textureSize();
        if (!m_outputTexture || m_outputTexture->pixelSize() != sz) {
            qCDebug(lcComputeRenderNode) << "ComputeRenderNode: Resizing output texture to" << sz;
            textureSizeChanged = true;
        }
    }

    bool needsRebuild = m_bufferLayoutDirty || textureSizeChanged;
    if (!needsRebuild)
        return m_vbuf != nullptr && m_ubuf != nullptr && m_sampler != nullptr;

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
    if (m_paramUbuf) {
        delete m_paramUbuf;
        m_paramUbuf = nullptr;
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
    m_renderTexture = nullptr;
    m_verticesUploaded = false;

    QSize sz(qMax(1, static_cast<int>(m_width)), qMax(1, static_cast<int>(m_height)));
    if (m_inputTexture) {
        QSize ts = m_inputTexture->textureSize();
        if (ts.isValid() && ts.width() > 0)
            sz = ts;
    }

    if (computeSupported)
        m_srb = rhi->newShaderResourceBindings();
    QList<QRhiShaderResourceBinding> bindings;

    if (true) {
        if (!m_sampler) {
            m_sampler = rhi->newSampler(QRhiSampler::Linear, QRhiSampler::Linear, QRhiSampler::None, QRhiSampler::ClampToEdge, QRhiSampler::ClampToEdge);
            m_sampler->create();
        }

        if (computeSupported && m_srb && m_inputRhiTexture) {
            bindings.append(QRhiShaderResourceBinding::sampledTexture(kInputBinding, QRhiShaderResourceBinding::ComputeStage, m_inputRhiTexture, m_sampler));
        }

        if (computeSupported && m_srb) {
            // RGBA8 から RGBA16F (16bit float) へ変更
            m_outputTexture = rhi->newTexture(QRhiTexture::RGBA16F, sz, 1, QRhiTexture::UsedWithLoadStore | QRhiTexture::RenderTarget);
            if (!m_outputTexture->create()) {
                delete m_outputTexture;
                m_outputTexture = nullptr;
                delete m_srb;
                m_srb = nullptr;
                m_error = QStringLiteral("Compute output texture creation failed.");
            } else {
                bindings.append(QRhiShaderResourceBinding::imageLoadStore(kOutputBinding, QRhiShaderResourceBinding::ComputeStage, m_outputTexture, 0));
            }
        }

        m_vbuf = rhi->newBuffer(QRhiBuffer::Immutable, QRhiBuffer::VertexBuffer, sizeof(kQuadData));
        if (!m_vbuf->create()) {
            m_error = QStringLiteral("Compute blit vertex buffer creation failed.");
            return false;
        }

        m_ubuf = rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, 64);
        if (!m_ubuf->create()) {
            m_error = QStringLiteral("Compute blit uniform buffer creation failed.");
            return false;
        }

        if (m_inputTexture && !m_inputRhiTexture) {
            m_bufferLayoutDirty = true;
        }
    }

    if (computeSupported && m_srb) {
        quint32 ubufSize = kParamsBlockSize;
        if (m_shader.isValid()) {
            const QShaderDescription desc = m_shader.description();
            const QList<QShaderDescription::UniformBlock> blocks = desc.uniformBlocks();
            for (const auto &block : blocks) {
                if (block.binding == kParamsBinding) {
                    ubufSize = block.size;
                    break;
                }
            }
        }
        ubufSize = qMax<quint32>(32, ubufSize);

        m_paramUbuf = rhi->newBuffer(QRhiBuffer::Dynamic, QRhiBuffer::UniformBuffer, ubufSize);
        if (!m_paramUbuf->create()) {
            delete m_paramUbuf;
            m_paramUbuf = nullptr;
            delete m_srb;
            m_srb = nullptr;
            if (m_error.isEmpty())
                m_error = QStringLiteral("Compute parameter uniform buffer creation failed.");
        } else {
            bindings.append(QRhiShaderResourceBinding::uniformBuffer(kParamsBinding, QRhiShaderResourceBinding::ComputeStage, m_paramUbuf));
            m_srb->setBindings(bindings.cbegin(), bindings.cend());
            if (!m_srb->create()) {
                delete m_srb;
                m_srb = nullptr;
                if (m_error.isEmpty())
                    m_error = QStringLiteral("Compute shader resource bindings creation failed.");
                qCWarning(lcComputeRenderNode) << m_error;
            }
        }
    }

    m_bufferLayoutDirty = false;
    m_shaderDirty = true;
    m_renderTargetDirty = true;
    return true;
}

bool ComputeRenderNode::ensurePipeline(QRhi *rhi) {
    QRhiTexture *wantedRenderTexture = (m_pipeline && m_inputRhiTexture) ? m_outputTexture : m_inputRhiTexture;
    if (!wantedRenderTexture && m_outputTexture)
        wantedRenderTexture = m_outputTexture;

    const bool renderTargetChanged = (m_renderTexture != wantedRenderTexture);
    if (!m_shaderDirty && !m_renderTargetDirty && !renderTargetChanged && m_pipeline && m_renderPipeline)
        return true;

    if (m_pipeline)
        delete m_pipeline;
    m_pipeline = nullptr;

    if (m_renderPipeline)
        delete m_renderPipeline;
    m_renderPipeline = nullptr;

    if (m_renderSrb) {
        delete m_renderSrb;
        m_renderSrb = nullptr;
    }
    m_renderTexture = wantedRenderTexture;
    m_error.clear();

    auto *ri = m_window->rendererInterface();

    bool computeOk = false;
    if (!rhi->isFeatureSupported(QRhi::Compute)) {
        m_error = QStringLiteral("Compute shaders are not supported on this hardware/backend.");
    } else if (!m_inputRhiTexture) {
        m_error = QStringLiteral("Compute input texture is not ready.");
    } else if (m_srb && m_shader.isValid()) {
        m_pipeline = rhi->newComputePipeline();
        m_pipeline->setShaderStage({QRhiShaderStage::Compute, m_shader});
        m_pipeline->setShaderResourceBindings(m_srb);
        computeOk = m_pipeline->create();
        if (!computeOk) {
            delete m_pipeline;
            m_pipeline = nullptr;
            m_error = QStringLiteral("Compute pipeline creation failed: %1").arg(m_shaderPath);
        }
    } else {
        m_error = QStringLiteral("Compute shader path or resource bindings are not ready.");
    }

    wantedRenderTexture = computeOk ? m_outputTexture : m_inputRhiTexture;
    if (!wantedRenderTexture && m_outputTexture)
        wantedRenderTexture = m_outputTexture;
    m_renderTexture = wantedRenderTexture;

    bool graphicsOk = false;
    if (m_renderTexture && m_ubuf && m_sampler) {
        m_renderSrb = rhi->newShaderResourceBindings();
        m_renderSrb->setBindings({
            QRhiShaderResourceBinding::uniformBuffer(0, QRhiShaderResourceBinding::VertexStage, m_ubuf),
            QRhiShaderResourceBinding::sampledTexture(1, QRhiShaderResourceBinding::FragmentStage, m_renderTexture, m_sampler),
        });
        if (!m_renderSrb->create()) {
            if (m_error.isEmpty())
                m_error = QStringLiteral("Compute blit shader resource bindings creation failed.");
            return false;
        }

        m_renderPipeline = rhi->newGraphicsPipeline();
        m_renderPipeline->setTopology(QRhiGraphicsPipeline::TriangleStrip);
        m_renderPipeline->setShaderResourceBindings(m_renderSrb);
        QRhiVertexInputLayout inputLayout;
        inputLayout.setBindings({{4 * sizeof(float)}});
        inputLayout.setAttributes({{0, 0, QRhiVertexInputAttribute::Float2, 0}, {0, 1, QRhiVertexInputAttribute::Float2, 2 * sizeof(float)}});
        m_renderPipeline->setVertexInputLayout(inputLayout);

        QString baseDir = QCoreApplication::applicationDirPath();
        if (baseDir.endsWith("/bin"))
            baseDir.chop(4);
        QString shaderDir = baseDir + "/common/shaders/";
        QFile vfile(shaderDir + "blit.vert.qsb"), ffile(shaderDir + "blit.frag.qsb");
        if (vfile.open(QIODevice::ReadOnly) && ffile.open(QIODevice::ReadOnly)) {
            m_renderPipeline->setShaderStages({{QRhiShaderStage::Vertex, QShader::fromSerialized(vfile.readAll())}, {QRhiShaderStage::Fragment, QShader::fromSerialized(ffile.readAll())}});
            auto *rt = renderTarget() ? renderTarget()->renderPassDescriptor() : nullptr;
            m_renderPipeline->setTargetBlends({{}});
            m_renderPipeline->setRenderPassDescriptor(rt);
            graphicsOk = m_renderPipeline->create();
            if (!graphicsOk && m_error.isEmpty())
                m_error = QStringLiteral("Compute blit graphics pipeline creation failed.");
        } else if (m_error.isEmpty()) {
            m_error = QStringLiteral("Compute blit shader files cannot be opened.");
        }
    }

    m_shaderDirty = false;
    m_renderTargetDirty = false;
    qCDebug(lcComputeRenderNode) << (graphicsOk ? (computeOk ? "Compute/Graphics" : "Graphics Only") : "Compute Only") << "パイプライン構築完了:" << m_shaderPath;
    return graphicsOk;
}

void ComputeRenderNode::prepare() {
    m_rhi = resolveRhi();
    if (!m_rhi)
        return;

    if (!ensureBuffers(m_rhi))
        return;
    if (!ensurePipeline(m_rhi))
        return;

    auto *cb = resolveCommandBuffer();
    if (!cb)
        return;

    QRhiResourceUpdateBatch *batch = m_rhi->nextResourceUpdateBatch();

    if (m_vbuf && !m_verticesUploaded) {
        batch->uploadStaticBuffer(m_vbuf, kQuadData);
        m_verticesUploaded = true;
    }

    static bool firstDispatch = true;
    if (firstDispatch || m_shaderDirty) {
        qCDebug(lcComputeRenderNode) << "ComputeRenderNode: Dispatching compute shader" << m_workGroupX << "x" << m_workGroupY;
        firstDispatch = false;
    }

    if (m_ubuf) {
        QMatrix4x4 mvp;
        if (const QMatrix4x4 *projection = projectionMatrix())
            mvp = *projection;
        if (const QMatrix4x4 *nodeMatrix = matrix())
            mvp *= *nodeMatrix;
        mvp.scale(m_width, m_height, 1.0f);
        batch->updateDynamicBuffer(m_ubuf, 0, 64, mvp.constData());
    }

    if (m_paramUbuf && m_shader.isValid()) {
        const QShaderDescription desc = m_shader.description();
        const QList<QShaderDescription::UniformBlock> blocks = desc.uniformBlocks();
        const QShaderDescription::UniformBlock *paramBlock = nullptr;
        for (const auto &block : blocks) {
            if (block.binding == kParamsBinding) {
                paramBlock = &block;
                break;
            }
        }

        if (paramBlock) {
            QByteArray upload(paramBlock->size, 0);
            for (const auto &member : paramBlock->members) {
                QVariant val = m_params.value(member.name);
                if (!val.isValid()) {
                    continue;
                }

                int offset = member.offset;
                if (offset + member.size > upload.size())
                    continue;

                switch (member.type) {
                case QShaderDescription::Float: {
                    float f = val.toFloat();
                    std::memcpy(upload.data() + offset, &f, sizeof(float));
                    break;
                }
                case QShaderDescription::Int: {
                    int i = val.toInt();
                    std::memcpy(upload.data() + offset, &i, sizeof(int));
                    break;
                }
                case QShaderDescription::Bool: {
                    int b = val.toBool() ? 1 : 0;
                    std::memcpy(upload.data() + offset, &b, sizeof(int));
                    break;
                }
                case QShaderDescription::Vec2: {
                    float v[2] = {0.0f, 0.0f};
                    if (val.canConvert<QPointF>()) {
                        QPointF p = val.toPointF();
                        v[0] = static_cast<float>(p.x());
                        v[1] = static_cast<float>(p.y());
                    } else if (val.canConvert<QVariantList>()) {
                        QVariantList list = val.toList();
                        if (list.size() >= 2) {
                            v[0] = list[0].toFloat();
                            v[1] = list[1].toFloat();
                        }
                    }
                    std::memcpy(upload.data() + offset, v, 2 * sizeof(float));
                    break;
                }
                case QShaderDescription::Vec3: {
                    float v[3] = {0.0f, 0.0f, 0.0f};
                    if (val.canConvert<QColor>()) {
                        QColor c = val.value<QColor>();
                        v[0] = static_cast<float>(c.redF());
                        v[1] = static_cast<float>(c.greenF());
                        v[2] = static_cast<float>(c.blueF());
                    } else if (val.canConvert<QVariantList>()) {
                        QVariantList list = val.toList();
                        if (list.size() >= 3) {
                            v[0] = list[0].toFloat();
                            v[1] = list[1].toFloat();
                            v[2] = list[2].toFloat();
                        }
                    }
                    std::memcpy(upload.data() + offset, v, 3 * sizeof(float));
                    break;
                }
                case QShaderDescription::Vec4: {
                    float v[4] = {0.0f, 0.0f, 0.0f, 0.0f};
                    if (val.canConvert<QColor>()) {
                        QColor c = val.value<QColor>();
                        v[0] = static_cast<float>(c.redF());
                        v[1] = static_cast<float>(c.greenF());
                        v[2] = static_cast<float>(c.blueF());
                        v[3] = static_cast<float>(c.alphaF());
                    } else if (val.canConvert<QVariantList>()) {
                        QVariantList list = val.toList();
                        if (list.size() >= 4) {
                            v[0] = list[0].toFloat();
                            v[1] = list[1].toFloat();
                            v[2] = list[2].toFloat();
                            v[3] = list[3].toFloat();
                        }
                    }
                    std::memcpy(upload.data() + offset, v, 4 * sizeof(float));
                    break;
                }
                default:
                    break;
                }
            }

            if (upload.size() < m_paramUbuf->size())
                upload.resize(m_paramUbuf->size());
            batch->updateDynamicBuffer(m_paramUbuf, 0, static_cast<quint32>(qMin<qsizetype>(upload.size(), m_paramUbuf->size())), upload.constData());
        }
    }

    if (!m_pipeline || !m_srb || !m_inputRhiTexture) {
        cb->resourceUpdate(batch);
        return;
    }

    cb->beginComputePass(batch);
    cb->setComputePipeline(m_pipeline);
    cb->setShaderResources(m_srb);
    cb->dispatch(m_workGroupX, m_workGroupY, m_workGroupZ);
    cb->endComputePass();

    m_paramsDirty = false;
}

void ComputeRenderNode::render(const RenderState *state) {
    auto *cb = resolveCommandBuffer();
    if (!cb || !m_renderPipeline || !m_renderSrb || !m_vbuf || !m_ubuf || !m_renderTexture)
        return;

    cb->setGraphicsPipeline(m_renderPipeline);

    const float dpr = m_window->devicePixelRatio();
    cb->setViewport(QRhiViewport(0, 0, static_cast<float>(m_window->width()) * dpr, static_cast<float>(m_window->height()) * dpr));

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
    delete m_paramUbuf;
    m_paramUbuf = nullptr;
    m_renderTexture = nullptr;
    m_inputRhiTexture = nullptr;
    m_verticesUploaded = false;

    m_bufferLayoutDirty = true;
    m_shaderDirty = true;
    m_renderTargetDirty = true;
}

QSGRenderNode::StateFlags ComputeRenderNode::changedStates() const { return ViewportState | ScissorState | ColorState | BlendState | CullState; }

QSGRenderNode::RenderingFlags ComputeRenderNode::flags() const { return BoundedRectRendering; }

} // namespace AviQtl::UI::Effects
