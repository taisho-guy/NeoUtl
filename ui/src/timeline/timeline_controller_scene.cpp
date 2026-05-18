#include "effect_registry.hpp"
#include "selection_service.hpp"
#include "timeline_controller.hpp"
#include "timeline_service.hpp"
#include "transport_service.hpp"
#include <QtGlobal>

namespace AviQtl::UI {

auto TimelineController::scenes() const -> QVariantList { return m_timeline->scenes(); }

auto TimelineController::currentSceneId() const -> int { return m_timeline->currentSceneId(); }

void TimelineController::createScene(const QString &name) { m_timeline->createScene(name); }

void TimelineController::removeScene(int sceneId) { m_timeline->removeScene(sceneId); }

void TimelineController::switchScene(int sceneId) { m_timeline->switchScene(sceneId); }

void TimelineController::updateSceneSettings(int sceneId, const QString &name, int width, int height, double fps, int totalFrames, const QString &gridMode, double gridBpm, double gridOffset, int gridInterval, int gridSubdivision, bool enableSnap,
                                             int magneticSnapRange) {
    m_timeline->updateSceneSettings(sceneId, name, width, height, fps, totalFrames, gridMode, gridBpm, gridOffset, gridInterval, gridSubdivision, enableSnap, magneticSnapRange);
}

auto TimelineController::getSceneClips(int sceneId) const -> QVariantList {
    QVariantList list;
    const auto &clips = m_timeline->clips(sceneId);

    for (const auto &clip : clips) {
        QVariantMap map;
        map.insert(QStringLiteral("id"), clip.id);
        map.insert(QStringLiteral("sceneId"), clip.sceneId);
        map.insert(QStringLiteral("type"), clip.type);
        map.insert(QStringLiteral("startFrame"), clip.startFrame);
        map.insert(QStringLiteral("durationFrames"), clip.durationFrames);
        map.insert(QStringLiteral("layer"), clip.layer);

        // QMLソースの解決
        auto meta = AviQtl::Core::EffectRegistry::instance().getEffect(clip.type);
        if (!meta.qmlSource.isEmpty()) {
            map.insert(QStringLiteral("qmlSource"), meta.qmlSource);
        }

        // パラメータの収集 (エフェクトからフラット化)
        QVariantMap params;
        // 基本情報もparamsに入れておく
        params.insert(QStringLiteral("layer"), clip.layer);
        params.insert(QStringLiteral("startFrame"), clip.startFrame);
        params.insert(QStringLiteral("durationFrames"), clip.durationFrames);
        params.insert(QStringLiteral("id"), clip.id);

        for (auto *eff : clip.effects) {
            if (!eff->isEnabled()) {
                continue;
            }
            // キーフレーム評価を行わず生パラメータまたはデフォルト値を渡す
            // SceneObject内で時間に応じて評価されるため
            QVariantMap p = eff->params();
            for (auto it = p.begin(); it != p.end(); ++it) {
                params.insert(it.key(), it.value());
            }
        }
        map.insert(QStringLiteral("params"), params);

        // エフェクトモデルのポインタリストを直接渡す (QMLでの一貫性のため)
        QList<QObject *> effList;
        for (auto *eff : clip.effects) {
            effList.append(eff);
        }
        map.insert(QStringLiteral("effectModels"), QVariant::fromValue(effList));

        list.append(map);
    }
    return list;
}

auto TimelineController::getSceneInfo(int sceneId) const -> QVariantMap {
    for (const auto &s : m_timeline->getAllScenes()) {
        if (s.id == sceneId) {
            return {{"id", s.id}, {"name", s.name}, {"width", s.width}, {"height", s.height}, {"fps", s.fps}, {"totalFrames", s.totalFrames}};
        }
    }
    return {};
}

auto TimelineController::getSceneDuration(int sceneId) const -> int {
    for (const auto &s : m_timeline->getAllScenes()) {
        if (s.id == sceneId) {
            return s.totalFrames;
        }
    }
    return 0;
}

void TimelineController::updateViewport(double x, double y) {
    // このメソッドは、QMLのレンダリングタイマーから呼び出され、現在の表示範囲をC++側に通知します。
    // 将来的に、描画範囲外のクリップのレンダリング計算をスキップする等の最適化に使用できます。
    Q_UNUSED(x)
    Q_UNUSED(y)
} // NOLINT(bugprone-easily-swappable-parameters)

} // namespace AviQtl::UI