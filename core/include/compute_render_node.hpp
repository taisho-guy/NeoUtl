#include <rhi/qrhi.h>
#include <rhi/qshader.h>
#pragma once
// フェーズ8: ComputeEffect スタブを実 Vulkan RHI バインディングに昇格
// QSGRenderNode::prepare() で SSBO を QRhiBuffer に upload し Compute Shader を dispatch する
// フェーズ9 で出力テクスチャとの接続 (SceneGraph Compositor) を行う
#include <QByteArray>
#include <QList>
#include <QSGRenderNode>
#include <QString>

QT_FORWARD_DECLARE_CLASS(QQuickWindow)
QT_FORWARD_DECLARE_CLASS(QRhi)
QT_FORWARD_DECLARE_CLASS(QRhiCommandBuffer)

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
    void syncWorkGroupSize(int x, int y, int z = 1);

    // QSGRenderNode オーバーライド
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
    QRhiShaderResourceBindings *m_srb = nullptr;
    QRhiComputePipeline *m_pipeline = nullptr;
    QShader m_shader;

    // updatePaintNode でセットされる転送データ
    QList<SSBOEntry> m_pendingSSBOs;
    bool m_ssboDirty = false;

    QString m_shaderPath;
    bool m_shaderDirty = true;
    bool m_bufferLayoutDirty = true;

    int m_workGroupX = 1;
    int m_workGroupY = 1;
    int m_workGroupZ = 1;
};

} // namespace AviQtl::UI::Effects
