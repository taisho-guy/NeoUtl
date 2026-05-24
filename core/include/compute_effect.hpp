#pragma once
#include <QByteArray>
#include <QList>
#include <QMatrix4x4>
#include <QObject>
#include <QQuickItem>
#include <QSGNode>
#include <QUrl>
#include <QVariantMap>
#include <cstdint>

namespace AviQtl::UI::Effects {

// forward declare のみ: RHI ヘッダは compute_render_node.hpp に閉じ込める
class ComputeRenderNode;

// QML-exposed C++ class for Shader Storage Buffer Object / Vulkan RHI / QSGRenderNode / GPU
class ComputeEffect : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT

    // 旧来プロパティ (Phase 6 以前)
    Q_PROPERTY(QQuickItem *source READ source WRITE setSource NOTIFY sourceChanged)
    Q_PROPERTY(QVariantMap params READ params WRITE setParams NOTIFY paramsChanged)
    Q_PROPERTY(QVariantMap storageBuffers READ storageBuffers WRITE setStorageBuffers NOTIFY storageBuffersChanged)
    Q_PROPERTY(bool shaderEnabled READ shaderEnabled WRITE setShaderEnabled NOTIFY shaderEnabledChanged)
    Q_PROPERTY(QUrl shaderPath READ shaderPath WRITE setShaderPath NOTIFY shaderPathChanged)

    Q_PROPERTY(QUrl computeShader READ shaderPath WRITE setShaderPath NOTIFY shaderPathChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(int workGroupSizeX READ workGroupSizeX WRITE setWorkGroupSizeX NOTIFY workGroupSizeXChanged)
    Q_PROPERTY(int workGroupSizeY READ workGroupSizeY WRITE setWorkGroupSizeY NOTIFY workGroupSizeYChanged)
    Q_PROPERTY(bool autoWorkGroup READ autoWorkGroup WRITE setAutoWorkGroup NOTIFY autoWorkGroupChanged)

  public:
    explicit ComputeEffect(QQuickItem *parent = nullptr);
    ~ComputeEffect() override;

    QQuickItem *source() const { return m_source; }
    void setSource(QQuickItem *s);

    QVariantMap params() const { return m_params; }
    QVariantMap storageBuffers() const { return m_storageBuffers; }
    bool shaderEnabled() const { return m_enabled; }
    QUrl shaderPath() const { return m_shaderPath; }
    QString error() const { return m_error; }

    int workGroupSizeX() const { return m_workGroupX; }
    int workGroupSizeY() const { return m_workGroupY; }
    bool autoWorkGroup() const { return m_autoWorkGroup; }

    void setParams(const QVariantMap &params);
    void setStorageBuffers(const QVariantMap &buffers);
    void setShaderEnabled(bool enabled);
    void setShaderPath(const QUrl &path);

    void setWorkGroupSizeX(int x);
    void setWorkGroupSizeY(int y);
    void setAutoWorkGroup(bool autoWG);

    Q_INVOKABLE void setStorageBufferRaw(const QString &name, int binding, const void *data, qsizetype byteSize);

  signals:
    void sourceChanged();
    void paramsChanged();
    void storageBuffersChanged();
    void shaderEnabledChanged();
    void shaderPathChanged();
    void errorChanged();
    void workGroupSizeXChanged();
    void workGroupSizeYChanged();
    void autoWorkGroupChanged();

  protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *data) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;

  private:
    void recalcAutoWorkGroup();

    // SSBO エントリ (setStorageBufferRaw → updatePaintNode → ComputeRenderNode の転送経路)
    struct RawSSBOEntry {
        QString name;
        int binding;
        QByteArray data;
    };
    static QByteArray ssboToBytes(const QVariantMap &bufferData);

    QQuickItem *m_source = nullptr;
    QVariantMap m_params;
    QVariantMap m_storageBuffers;
    bool m_enabled = true;
    QUrl m_shaderPath;
    QString m_error;
    bool m_dirty = false;

    int m_workGroupX = 1;
    int m_workGroupY = 1;
    bool m_autoWorkGroup = true;

    QList<RawSSBOEntry> m_rawSSBOs;
};

} // namespace AviQtl::UI::Effects
