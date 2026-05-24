#pragma once
#include <QByteArray>
#include <QList>
#include <QSGRenderNode>
#include <QSGTexture>
#include <QString>
#include <rhi/qrhi.h>
#include <rhi/qshader.h>

QT_FORWARD_DECLARE_CLASS(QQuickWindow)

namespace AviQtl::UI::Effects {

class ComputeRenderNode final : public QSGRenderNode {
  public:
    // ComputeEffect (UI スレッド) → ComputeRenderNode (レンダースレッド) の転送単位
    // updatePaintNode から syncSSBOs() に渡す。UI スレッドブロック中に呼ばれるため mutex 不要
    struct SSBOEntry {
        int binding = 0;
        QByteArray data;
    };

    explicit ComputeRenderNode(QQuickWindow *window);
    ~ComputeRenderNode() override;

    // updatePaintNode から呼ぶ同期 API (UI スレッドブロック保証)
    void syncSSBOs(const QList<SSBOEntry> &entries);
    void syncShaderPath(const QString &path);
    void syncInputTexture(QSGTexture *tex);
    void syncSize(float w, float h);
    void syncWorkGroupSize(int x, int y, int z = 1);

    // QSGRenderNode オーバーライド
    QRectF rect() const override;
    void prepare() override;
    void render(const RenderState *state) override;
    void releaseResources() override;
    StateFlags changedStates() const override;
    RenderingFlags flags() const override;

  private:
    // GPU バッファの管理単位 (binding → QRhiBuffer の 1 対 1 対応)
    struct GpuBuffer {
        int binding = 0;
        QRhiBuffer *buf = nullptr;
        qsizetype size = 0;
    };

    // QRhi / CommandBuffer の取得ヘルパー
    QRhi *resolveRhi() const;
    QRhiCommandBuffer *resolveCommandBuffer() const;

    // リソース構築 (prepare() 内で段階的に呼び出す)
    bool ensureBuffers(QRhi *rhi);
    bool ensurePipeline(QRhi *rhi);
    void destroyResources();

    QQuickWindow *m_window = nullptr;
    QRhi *m_rhi = nullptr;

    QList<GpuBuffer> m_gpuBuffers;
    QRhiTexture *m_outputTexture = nullptr;
    QRhiSampler *m_sampler = nullptr;
    QRhiBuffer *m_vbuf = nullptr;
    QRhiBuffer *m_ubuf = nullptr;
    QRhiShaderResourceBindings *m_renderSrb = nullptr;
    QRhiGraphicsPipeline *m_renderPipeline = nullptr;

    QRhiShaderResourceBindings *m_srb = nullptr;
    QRhiComputePipeline *m_pipeline = nullptr;
    QShader m_shader;

    // updatePaintNode でセットされる転送データ
    QList<SSBOEntry> m_pendingSSBOs;
    bool m_ssboDirty = false;

    QString m_shaderPath;
    float m_width = 0;
    float m_height = 0;
    QSGTexture *m_inputTexture = nullptr;

    bool m_shaderDirty = true;
    bool m_bufferLayoutDirty = true;

    int m_workGroupX = 1;
    int m_workGroupY = 1;
    int m_workGroupZ = 1;
};

} // namespace AviQtl::UI::Effects
