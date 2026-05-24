#include "compute_effect.hpp"
#include "compute_render_node.hpp"
#include <QDebug>
#include <QLoggingCategory>
#include <QSGNode>
#include <QSGTexture>
#include <QSGTextureProvider>
#include <QUrl>
#include <algorithm>
#include <cmath>
#include <cstring>

namespace AviQtl::UI::Effects {

ComputeEffect::ComputeEffect(QQuickItem *parent) : QQuickItem(parent) { setFlag(ItemHasContents, true); }

ComputeEffect::~ComputeEffect() = default;

void ComputeEffect::setSource(QQuickItem *s) {
    if (m_source == s)
        return;
    m_source = s;
    m_dirty = true;
    emit sourceChanged();
    update();
}

void ComputeEffect::setParams(const QVariantMap &params) {
    if (m_params == params)
        return;
    m_params = params;
    m_dirty = true;
    emit paramsChanged();
    update();
}

void ComputeEffect::setStorageBuffers(const QVariantMap &buffers) {
    if (m_storageBuffers == buffers)
        return;
    m_storageBuffers = buffers;
    m_dirty = true;
    emit storageBuffersChanged();
    update();
}

void ComputeEffect::setShaderEnabled(bool enabled) {
    if (m_enabled == enabled)
        return;
    m_enabled = enabled;
    m_dirty = true;
    emit shaderEnabledChanged();
    update();
}

void ComputeEffect::setShaderPath(const QUrl &path) {
    if (m_shaderPath == path)
        return;
    m_shaderPath = path;
    m_dirty = true;
    emit shaderPathChanged();
    update();
}

void ComputeEffect::setWorkGroupSizeX(int x) {
    const int clamped = qMax(1, x);
    if (m_workGroupX == clamped)
        return;
    m_workGroupX = clamped;
    m_dirty = true;
    emit workGroupSizeXChanged();
    update();
}

void ComputeEffect::setWorkGroupSizeY(int y) {
    const int clamped = qMax(1, y);
    if (m_workGroupY == clamped)
        return;
    m_workGroupY = clamped;
    m_dirty = true;
    emit workGroupSizeYChanged();
    update();
}

void ComputeEffect::setAutoWorkGroup(bool autoWG) {
    if (m_autoWorkGroup == autoWG)
        return;
    m_autoWorkGroup = autoWG;
    if (m_autoWorkGroup)
        recalcAutoWorkGroup();
    m_dirty = true;
    emit autoWorkGroupChanged();
    update();
}

void ComputeEffect::setStorageBufferRaw(const QString &name, int binding, const void *data, qsizetype byteSize) {
    for (auto &entry : m_rawSSBOs) {
        if (entry.name == name) {
            if (entry.data.size() == byteSize && std::memcmp(entry.data.constData(), data, static_cast<std::size_t>(byteSize)) == 0)
                return;
            entry.binding = binding;
            entry.data = QByteArray(static_cast<const char *>(data), static_cast<qsizetype>(byteSize));
            m_dirty = true;
            update();
            return;
        }
    }
    m_rawSSBOs.push_back(RawSSBOEntry{name, binding, QByteArray(static_cast<const char *>(data), static_cast<qsizetype>(byteSize))});
    m_dirty = true;
    update();
}

void ComputeEffect::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (m_autoWorkGroup)
        recalcAutoWorkGroup();
}

// 16x16 スレッドを 1 ワークグループとして item の縦横スレッド数を算出する
// アイテム幅/高さが 0 の場合はデフォルト (1x1) を維持する
void ComputeEffect::recalcAutoWorkGroup() {
    if (width() > 0.0) {
        const int newX = qMax(1, static_cast<int>(std::ceil(width() / 16.0)));
        if (m_workGroupX != newX) {
            m_workGroupX = newX;
            emit workGroupSizeXChanged();
        }
    }
    if (height() > 0.0) {
        const int newY = qMax(1, static_cast<int>(std::ceil(height() / 16.0)));
        if (m_workGroupY != newY) {
            m_workGroupY = newY;
            emit workGroupSizeYChanged();
        }
    }
}

// QVariantMap → バイト列変換 (旧来の互換パス。フェーズ6以降は setStorageBufferRaw を優先)
auto ComputeEffect::ssboToBytes(const QVariantMap &bufferData) -> QByteArray {
    QByteArray result;
    result.reserve(static_cast<qsizetype>(bufferData.size()) * static_cast<qsizetype>(sizeof(float)));
    for (auto it = bufferData.constBegin(); it != bufferData.constEnd(); ++it) {
        const QVariant &v = it.value();
        if (v.canConvert<float>()) {
            float f = v.toFloat();
            result.append(reinterpret_cast<const char *>(&f), sizeof(float));
        } else if (v.canConvert<int>()) {
            int i = v.toInt();
            result.append(reinterpret_cast<const char *>(&i), sizeof(int));
        }
    }
    return result;
}

auto ComputeEffect::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) -> QSGNode * {
    if (!m_enabled) {
        delete oldNode;
        return nullptr;
    }

    auto *node = static_cast<ComputeRenderNode *>(oldNode);
    if (!node) {
        node = new ComputeRenderNode(window());
        m_dirty = true;
    }

    // m_dirty だけでなく、ソース（動画・画像）がある場合は毎フレーム更新を試みる
    // これにより動画再生中のテクスチャ更新がノードへ伝播する
    if (m_dirty || m_source) {
        // m_rawSSBOs → ComputeRenderNode::SSBOEntry に変換して転送
        QList<ComputeRenderNode::SSBOEntry> entries;
        if (!m_rawSSBOs.isEmpty()) {
            entries.reserve(m_rawSSBOs.size());
            for (const auto &raw : std::as_const(m_rawSSBOs))
                entries.push_back({raw.binding, raw.data});
        }

        // m_rawSSBOs が空の場合、params (m_params) を Binding 0 として自動転送
        if (entries.isEmpty() && !m_params.isEmpty()) {
            const QByteArray bytes = ssboToBytes(m_params);
            if (!bytes.isEmpty())
                entries.push_back({0, bytes});
        } else if (entries.isEmpty() && !m_storageBuffers.isEmpty()) {
            // 旧来 QVariantMap パス: m_rawSSBOs が空の場合のみ使用する
            for (auto it = m_storageBuffers.cbegin(); it != m_storageBuffers.cend(); ++it) {
                const QByteArray bytes = ssboToBytes(it.value().toMap());
                if (!bytes.isEmpty())
                    entries.push_back({0, bytes});
            }
        }

        // ソーステクスチャの同期
        if (m_source) {
            // updatePaintNode はレンダースレッドで呼ばれるため、テクスチャプロバイダーから安全にテクスチャを取得可能
            QSGTextureProvider *provider = m_source->textureProvider();
            if (!provider) {
                static int pWarn = 0;
                if (pWarn++ % 60 == 0)
                    qCWarning(lcComputeRenderNode) << "ComputeEffect: Source item" << m_source << "has NO texture provider!";
            }
            QSGTexture *tex = provider ? provider->texture() : nullptr;

            qCDebug(lcComputeRenderNode) << "ComputeEffect: Syncing texture" << tex << "from source" << m_source->objectName() << "to node";
            node->syncInputTexture(tex);

            if (!tex) {
                static int warnCount = 0;
                if (warnCount++ % 60 == 0)
                    qCWarning(lcComputeRenderNode) << "ComputeEffect: tex is NULL. Item exists but has no GPU texture. Is layer.enabled: true?";
            }
        } else {
            node->syncInputTexture(nullptr);
        }

        node->syncSSBOs(entries);
        // QUrl が file:// 形式ならローカルパスへ、そうでなければ(qrc等)そのまま文字列へ
        QString pathStr = m_shaderPath.isLocalFile() ? m_shaderPath.toLocalFile() : m_shaderPath.toString();
        node->syncShaderPath(pathStr);
        node->syncSize(width(), height());
        node->syncWorkGroupSize(m_workGroupX, m_workGroupY);
        m_dirty = false;
    }

    // レンダースレッド側で発生したエラーを QML プロパティへ同期する
    QString nodeErr = node->errorMessage();
    if (m_error != nodeErr) {
        m_error = nodeErr;
        emit errorChanged();
    }

    // QSGNode のダーティフラグを立てて prepare() / render() が呼ばれることを保証する
    node->markDirty(QSGNode::DirtyMaterial);
    return node;
}

} // namespace AviQtl::UI::Effects
