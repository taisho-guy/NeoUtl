#include "bake_controller.hpp"
#include "core/include/document_model.hpp"
#include "core/include/settings_manager.hpp"
#include "ecs.hpp"
#include <algorithm>
#include <bitset>

namespace AviQtl::Engine::Timeline {

BakeController::BakeController() {
    // DocumentModelの構造変更（クリップ追加・削除・エフェクト増減）を監視してBakeをトリガーする
    connect(&AviQtl::Core::DocumentModel::instance(), &AviQtl::Core::DocumentModel::structureChanged, this, &BakeController::onStructureChanged);
}

BakeController &BakeController::instance() {
    static BakeController inst;
    return inst;
}

void BakeController::bake(int sceneId, int currentFrame) {
    const auto *scene = AviQtl::Core::DocumentModel::instance().findScene(sceneId);
    if (!scene)
        return;

    auto &sm = AviQtl::Core::SettingsManager::instance();
    const QString strategy = sm.value(QStringLiteral("bakeStrategy"), QStringLiteral("FullBake")).toString();
    const int prefetch = sm.value(QStringLiteral("onDemandPrefetchFrames"), 30).toInt();

    const bool isFullBake = (strategy == QStringLiteral("FullBake"));
    std::bitset<MAX_CLIP_ID> aliveFlags;

    for (const auto &clip : scene->clips) {
        if (clip.id < 0 || clip.id >= MAX_CLIP_ID)
            continue;

        bool shouldBake = false;
        if (isFullBake) {
            shouldBake = true;
        } else {
            // OnDemand モード: クリップが [currentFrame - prefetch, currentFrame + prefetch] の範囲と重なっているか
            const int start = clip.startFrame;
            const int end = clip.startFrame + clip.durationFrames;
            const int rangeStart = currentFrame - prefetch;
            const int rangeEnd = currentFrame + prefetch;
            if (start <= rangeEnd && end >= rangeStart) {
                shouldBake = true;
            }
        }

        if (shouldBake) {
            aliveFlags.set(static_cast<std::size_t>(clip.id));

            // ECS状態に初期ベイク
            // (キーフレーム評価・補間ランタイムはPhase 4以降 ECS::InterpolationSystem が内部で実行するため、
            //  Bake時には位置・尺などの基本属性の登録のみを行う)
            const double relTime = static_cast<double>(std::max(0, currentFrame - clip.startFrame));
            ECS::instance().updateClipState(clip.id, clip.layer, relTime, clip.startFrame, clip.durationFrames);

            if (clip.type == QStringLiteral("audio") || clip.type == QStringLiteral("video")) {
                // 音声初期値
                ECS::instance().updateAudioClipState(clip.id, clip.startFrame, clip.durationFrames, 1.0f, 0.0f, false);
            }
        }
    }

    // ECS側に生存エンティティIDを同期させて、範囲外・削除済みのエンティティを消去する
    ECS::instance().syncClipIds(aliveFlags);
    ECS::instance().commit();

    m_lastSceneId = sceneId;
    m_lastFrame = currentFrame;
}

void BakeController::triggerRebake() {
    if (m_lastSceneId != -1) {
        bake(m_lastSceneId, m_lastFrame != -1 ? m_lastFrame : 0);
    }
}

void BakeController::onStructureChanged() { triggerRebake(); }

} // namespace AviQtl::Engine::Timeline
