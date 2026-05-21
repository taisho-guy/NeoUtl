#include "audio_decoder.hpp"
#include "commands.hpp"
#include "effect_registry.hpp"
#include "engine/plugin/audio_plugin_manager.hpp"
#include "selection_service.hpp"
#include "settings_manager.hpp"
#include "timeline_controller.hpp"
#include "timeline_service.hpp"
#include "transport_service.hpp"
#include "video_decoder.hpp"
#include <QtGlobal>
#include <algorithm>

namespace AviQtl::UI {

void TimelineController::handleClipClick(int clipId, int modifiers) { // NOLINT(bugprone-easily-swappable-parameters)
    if ((modifiers & Qt::ControlModifier) != 0U) {
        m_timeline->toggleSelection(clipId, QVariantMap());
    } else {
        m_timeline->applySelectionIds({clipId});
    }
}

void TimelineController::updateSelectionPreview(int frameA, int frameB, int layerA, int layerB, bool additive) { // NOLINT(bugprone-easily-swappable-parameters)
    QVariantList ids;
    if (additive && (m_selection != nullptr)) {
        ids = m_selection->selectedClipIds();
    }

    int minF = std::min(frameA, frameB);
    int maxF = std::max(frameA, frameB);
    int minL = std::min(layerA, layerB);
    int maxL = std::max(layerA, layerB);

    for (const auto &clip : m_timeline->clips()) {
        // GroupControlによるレイヤーの拡張分を考慮
        int groupLayerCount = 0;
        for (auto *eff : clip.effects) {
            if (eff->id() == QLatin1String("GroupControl")) {
                groupLayerCount = eff->params().value(QStringLiteral("layerCount"), 0).toInt();
                break;
            }
        }
        int clipMaxL = clip.layer + groupLayerCount;

        int clipEnd = clip.startFrame + clip.durationFrames;
        if (clip.startFrame < maxF && minF < clipEnd && clipMaxL >= minL && clip.layer <= maxL) {
            if (!ids.contains(clip.id)) {
                ids.append(clip.id);
            }
        }
    }

    if (m_previewSelectionIds != ids) {
        m_previewSelectionIds = ids;
        emit previewSelectionIdsChanged();
    }
}

void TimelineController::finalizeSelectionPreview() {
    applySelectionIds(m_previewSelectionIds);
    clearSelectionPreview();
}

void TimelineController::clearSelectionPreview() {
    if (!m_previewSelectionIds.isEmpty()) {
        m_previewSelectionIds.clear();
        emit previewSelectionIdsChanged();
    }
}

auto TimelineController::previewSelectionIds() const -> QVariantList { return m_previewSelectionIds; }

void TimelineController::setClipProperty(const QString &name, const QVariant &value) {
    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    m_timeline->undoStack()->beginMacro(tr("プロパティ変更: %1").arg(name));

    for (const QVariant &vId : ids) {
        int id = vId.toInt();
        const ClipData *clip = m_timeline->findClipById(id);
        if (clip == nullptr) {
            continue;
        }

        int targetEffectIndex = -1;
        for (int i = 0; i < clip->effects.size(); ++i) {
            if (clip->effects.value(i)->params().contains(name)) {
                targetEffectIndex = i;
                break;
            }
        }

        if (targetEffectIndex == -1 && !clip->effects.isEmpty()) {
            targetEffectIndex = 0;
            static const QStringList transformKeys = {"x", "y", "z", "scale", "aspect", "rotationX", "rotationY", "rotationZ", "opacity"};
            if (!transformKeys.contains(name) && clip->effects.size() > 1) {
                targetEffectIndex = 1;
            }
        }

        if (targetEffectIndex != -1 && targetEffectIndex < clip->effects.size()) {
            updateClipEffectParam(id, targetEffectIndex, name, value);
        }
    }

    m_timeline->undoStack()->endMacro();
}

auto TimelineController::getClipProperty(const QString &name) const -> QVariant { return m_selection->selectedClipData().value(name); }

auto TimelineController::clipStartFrame() const -> int { return m_selection->selectedClipData().value(QStringLiteral("startFrame"), 0).toInt(); }
void TimelineController::setClipStartFrame(int frame) {
    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    m_timeline->undoStack()->beginMacro(tr("開始フレーム変更"));
    for (const QVariant &vId : ids) {
        int id = vId.toInt();
        if (const auto *c = m_timeline->findClipById(id)) {
            m_timeline->updateClip(id, c->layer, frame, c->durationFrames);
        }
    }
    m_timeline->undoStack()->endMacro();
}

auto TimelineController::clipDurationFrames() const -> int { return m_selection->selectedClipData().value(QStringLiteral("durationFrames"), 100).toInt(); }
void TimelineController::setClipDurationFrames(int frames) {
    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    m_timeline->undoStack()->beginMacro(tr("長さ変更"));
    for (const QVariant &vId : ids) {
        int id = vId.toInt();
        if (const auto *c = m_timeline->findClipById(id)) {
            m_timeline->updateClip(id, c->layer, c->startFrame, frames);
        }
    }
    m_timeline->undoStack()->endMacro();
}

auto TimelineController::layer() const -> int { return m_selection->selectedClipData().value(QStringLiteral("layer"), 0).toInt(); }
void TimelineController::setLayer(int layer) {
    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    m_timeline->undoStack()->beginMacro(tr("レイヤー変更"));
    for (const QVariant &vId : ids) {
        int id = vId.toInt();
        if (const auto *c = m_timeline->findClipById(id)) {
            m_timeline->updateClip(id, layer, c->startFrame, c->durationFrames);
        }
    }
    m_timeline->undoStack()->endMacro();
}

void TimelineController::setSelectedLayer(int layer) {
    if (m_selectedLayer != layer) {
        m_selectedLayer = layer;
        emit selectedLayerChanged();
    }
}

auto TimelineController::isClipActive() const -> bool { return m_isClipActive; }

void TimelineController::updateClipActiveState() {
    int current = m_transport->currentFrame();
    int start = clipStartFrame();
    int duration = clipDurationFrames();
    // 矩形判定
    bool active = (current >= start) && (current < start + duration);
    if (m_isClipActive != active) {
        m_isClipActive = active;
        emit isClipActiveChanged();
    }
}

auto TimelineController::activeObjectType() const -> QString { return m_selection->selectedClipData().value(QStringLiteral("type"), "rect").toString(); }

void TimelineController::createObject(const QString &type, int startFrame, int layer) {
    if (m_timeline != nullptr) {
        m_timeline->createClip(type, startFrame, layer);
    }
}

auto TimelineController::getClipEffectsModel(int clipId) const -> QList<QObject *> {
    QList<QObject *> list;
    for (const auto &clip : m_timeline->clips()) {
        if (clip.id == clipId) {
            for (auto *eff : clip.effects) {
                list.append(eff);
            }
            break;
        }
    }
    return list;
}

void TimelineController::updateClipEffectParam(int clipId, int effectIndex, const QString &paramName, const QVariant &value) { m_timeline->updateEffectParam(clipId, effectIndex, paramName, value); }

auto TimelineController::clips() const -> QVariantList {
    QVariantList list;
    for (const auto &clip : m_timeline->clips()) {
        QVariantMap map;
        map.insert(QStringLiteral("id"), clip.id);
        map.insert(QStringLiteral("sceneId"), clip.sceneId);
        map.insert(QStringLiteral("type"), clip.type);
        map.insert(QStringLiteral("startFrame"), clip.startFrame);
        map.insert(QStringLiteral("durationFrames"), clip.durationFrames);
        map.insert(QStringLiteral("layer"), clip.layer);

        // オブジェクトのQMLパスを取得して追加
        auto meta = AviQtl::Core::EffectRegistry::instance().getEffect(clip.type);
        map.insert(QStringLiteral("name"), !meta.name.isEmpty() ? meta.name : clip.type);
        if (!meta.qmlSource.isEmpty()) {
            map.insert(QStringLiteral("qmlSource"), meta.qmlSource);
        }

        // params を構築して追加
        QVariantMap params;
        // 基本情報もparamsに入れておく（QML側での利便性とBaseObjectでの参照用）
        params.insert(QStringLiteral("layer"), clip.layer);
        params.insert(QStringLiteral("startFrame"), clip.startFrame);
        params.insert(QStringLiteral("durationFrames"), clip.durationFrames);
        params.insert(QStringLiteral("id"), clip.id);

        int groupLayerCount = 0;
        for (auto *eff : clip.effects) {
            if (eff->id() == QLatin1String("GroupControl")) {
                groupLayerCount = eff->params().value(QStringLiteral("layerCount"), 0).toInt();
                break;
            }
        }
        map.insert(QStringLiteral("groupLayerCount"), groupLayerCount);

        // エフェクトモデルのポインタリストを直接渡す (QMLでの一貫性のため)
        QList<QObject *> effList;
        for (auto *eff : clip.effects) {
            QVariantMap p = eff->params();
            for (auto it = p.begin(); it != p.end(); ++it) {
                params.insert(it.key(), it.value());
            }
            effList.append(eff);
        }
        map.insert(QStringLiteral("params"), params);
        map.insert(QStringLiteral("effectModels"), QVariant::fromValue(effList));

        list.append(map);
    }
    return list;
}

void TimelineController::moveSelectedClips(int deltaLayer, int deltaFrame) {
    if (m_timeline != nullptr) {
        m_timeline->moveSelectedClips(deltaLayer, deltaFrame);
    }
}

void TimelineController::applyClipBatchMove(const QVariantList &moves) {
    if (m_timeline != nullptr) {
        m_timeline->applyClipBatchMove(moves);
    }
}

void TimelineController::resizeSelectedClips(int deltaStartFrame, int deltaDuration) {
    if (m_timeline == nullptr) {
        return;
    }

    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    // リサイズ前の状態を値コピー（updateClip 呼び出しでポインタが失効しないよう）
    struct PendingResize {
        int id;
        int layer;
        int oldStart;
        int oldDuration;
    };

    QVector<PendingResize> pending;
    pending.reserve(ids.size());
    for (const QVariant &vId : std::as_const(ids)) {
        const int id = vId.toInt();
        const auto *clip = m_timeline->findClipById(id);
        if (clip == nullptr) {
            continue;
        }
        pending.push_back({id, clip->layer, clip->startFrame, clip->durationFrames});
    }
    if (pending.isEmpty()) {
        return;
    }

    // TimelineService と同一の衝突回避ソート順を維持する
    if (deltaStartFrame > 0 || deltaDuration > 0) {
        std::ranges::sort(pending, [](const PendingResize &a, const PendingResize &b) { return a.oldStart != b.oldStart ? a.oldStart > b.oldStart : a.layer > b.layer; });
    } else {
        std::ranges::sort(pending, [](const PendingResize &a, const PendingResize &b) { return a.oldStart != b.oldStart ? a.oldStart < b.oldStart : a.layer < b.layer; });
    }

    // updateClip() 経由で適用: メディア長クランプ・ECS 同期・Undo 登録を全クリップに保証
    m_timeline->undoStack()->beginMacro(tr("複数クリップリサイズ: %1").arg(pending.size()));
    for (const PendingResize &r : std::as_const(pending)) {
        const int newStart = std::max(0, r.oldStart + deltaStartFrame);
        const int newDuration = std::max(1, r.oldDuration + deltaDuration);
        updateClip(r.id, r.layer, newStart, newDuration);
    }
    m_timeline->undoStack()->endMacro();
}

// ----------------------------------------------------------------
// clampedDuration: video/audio/scene の素材長に合わせて duration を上限クランプ
// updateClip と resizeSelectedClips の共通ロジックとして抽出
// ----------------------------------------------------------------
int TimelineController::clampedDuration(int clipId, int newStart, int requestedDuration) const {
    Q_UNUSED(newStart);
    const auto *clip = m_timeline->findClipById(clipId);
    if (clip == nullptr) {
        return requestedDuration;
    }

    const int projectFps = static_cast<int>(project()->fps());
    int duration = requestedDuration;

    if (clip->type == QLatin1String("video")) {
        auto *vid = qobject_cast<AviQtl::Core::VideoDecoder *>(m_mediaManager->decoderForClip(clipId));
        if ((vid != nullptr) && vid->isReady()) {
            int startVideoFrame = 0;
            double speed = 100.0;
            bool isDirectMode = false;

            for (const auto *eff : clip->effects) {
                if (eff->id() != "video") {
                    continue;
                }
                const QString playMode = eff->params().value(QStringLiteral("playMode"), "開始フレーム＋再生速度").toString();
                if (playMode == QStringLiteral("フレーム直接指定")) {
                    isDirectMode = true;
                    break;
                }
                startVideoFrame = eff->params().value(QStringLiteral("startFrame"), 0).toInt();
                speed = eff->params().value(QStringLiteral("speed"), 100.0).toDouble();
                break;
            }

            double srcFps = vid->sourceFps();
            if (srcFps <= 0.0) {
                srcFps = projectFps;
            }
            int maxDuration = duration;

            if (isDirectMode) {
                const double totalSec = static_cast<double>(vid->totalFrameCount()) / srcFps;
                maxDuration = static_cast<int>(totalSec * projectFps);
            } else if (speed > 0.0) {
                const double startSec = static_cast<double>(startVideoFrame) / srcFps;
                const double remainingSec = (static_cast<double>(vid->totalFrameCount()) / srcFps) - startSec;
                if (remainingSec > 0.0) {
                    maxDuration = static_cast<int>(remainingSec / (speed / 100.0) * projectFps);
                }
            }
            if (maxDuration > 0 && duration > maxDuration) {
                duration = maxDuration;
            }
        }
    } else if (clip->type == QLatin1String("audio")) {
        auto *aud = qobject_cast<AviQtl::Core::AudioDecoder *>(m_mediaManager->decoderForClip(clipId));
        if ((aud != nullptr) && aud->isReady()) {
            double startTime = 0.0;
            double speed = 100.0;
            bool isDirectMode = false;

            for (const auto *eff : clip->effects) {
                if (eff->id() != "audio") {
                    continue;
                }
                const QString playMode = eff->params().value(QStringLiteral("playMode"), "開始時間＋再生速度").toString();
                if (playMode == QStringLiteral("時間直接指定")) {
                    isDirectMode = true;
                    break;
                }
                startTime = eff->params().value(QStringLiteral("startTime"), 0.0).toDouble();
                speed = eff->params().value(QStringLiteral("speed"), 100.0).toDouble();
                break;
            }

            const double totalSec = aud->totalDurationSec();
            int maxDuration = duration;

            if (isDirectMode) {
                maxDuration = static_cast<int>(totalSec * projectFps);
            } else if (speed > 0.0) {
                const double remainingSec = totalSec - startTime;
                if (remainingSec > 0.0) {
                    maxDuration = static_cast<int>(remainingSec / (speed / 100.0) * projectFps);
                }
            }
            if (maxDuration > 0 && duration > maxDuration) {
                duration = maxDuration;
            }
        }
    } else if (clip->type == QLatin1String("scene")) {
        int targetSceneId = 0;
        double speed = 1.0;
        int offset = 0;

        for (const auto *eff : clip->effects) {
            if (eff->id() != "scene") {
                continue;
            }
            targetSceneId = eff->params().value(QStringLiteral("targetSceneId"), 0).toInt();
            speed = eff->params().value(QStringLiteral("speed"), 1.0).toDouble();
            offset = eff->params().value(QStringLiteral("offset"), 0).toInt();
            break;
        }

        const int sceneDur = getSceneDuration(targetSceneId);
        if (sceneDur > 0 && speed > 0.0) {
            const double rhs = (static_cast<double>(sceneDur - 1 - offset)) / speed;
            int maxDuration = std::max(static_cast<int>(rhs) + 1, 1);
            duration = std::min(duration, maxDuration);
        }
    }

    return duration;
}

// ----------------------------------------------------------------
// updateClip: clampedDuration() に委譲して DRY を維持
// ----------------------------------------------------------------
void TimelineController::updateClip(int id, int layer, int startFrame, int duration) {
    const auto *clip = m_timeline->findClipById(id);
    if (clip == nullptr) {
        return;
    }

    const int clamped = clampedDuration(id, startFrame, duration);
    m_timeline->updateClip(id, layer, startFrame, clamped);
}



void TimelineController::selectClip(int id) {
    if (m_timeline != nullptr) {
        m_timeline->applySelectionIds(QVariantList{id});
    }
}

void TimelineController::toggleSelection(int id, const QVariantMap &data) {
    if (m_timeline != nullptr) {
        m_timeline->toggleSelection(id, data);
    }
}

void TimelineController::applySelectionIds(const QVariantList &ids) {
    if (m_timeline != nullptr) {
        m_timeline->applySelectionIds(ids);
    }
}

void TimelineController::addEffect(int clipId, const QString &effectId) {
    m_timeline->addEffect(clipId, effectId);
    updateActiveClipsList();
}

void TimelineController::removeEffect(int clipId, int effectIndex) {
    m_timeline->removeEffect(clipId, effectIndex);
    updateActiveClipsList();
}

void TimelineController::removeMultipleEffects(int clipId, const QList<int> &indices) {
    m_timeline->removeMultipleEffects(clipId, indices);
    updateActiveClipsList();
}

void TimelineController::setEffectEnabled(int clipId, int effectIndex, bool enabled) {
    if (m_timeline != nullptr) {
        m_timeline->setEffectEnabled(clipId, effectIndex, enabled);
    }
}

void TimelineController::reorderEffects(int clipId, int oldIndex, int newIndex) {
    if (m_timeline != nullptr) {
        m_timeline->reorderEffects(clipId, oldIndex, newIndex);
    }
}

void TimelineController::reorderMultipleEffects(int clipId, const QVariantList &indicesList, int targetIndex) {
    if (m_timeline != nullptr) {
        m_timeline->reorderMultipleEffects(clipId, indicesList, targetIndex);
    }
}

void TimelineController::copyEffect(int clipId, int effectIndex) { m_timeline->copyEffect(clipId, effectIndex); }

void TimelineController::pasteEffect(int clipId, int targetIndex) { m_timeline->pasteEffect(clipId, targetIndex); }

void TimelineController::cutEffect(int clipId, int effectIndex) {
    m_timeline->copyEffect(clipId, effectIndex);
    m_timeline->removeEffect(clipId, effectIndex);
}

void TimelineController::addAudioPlugin(int clipId, const QString &pluginId) {
    auto plugin = AviQtl::Engine::Plugin::AudioPluginManager::instance().createPlugin(pluginId);
    if (plugin) {
        qInfo() << "Adding audio plugin:" << plugin->name() << "to clip" << clipId;
        m_mediaManager->audioMixer()->getChain(clipId).add(std::move(plugin));
        emit clipEffectsChanged(clipId);
    } else {
        qWarning() << "Failed to create audio plugin:" << pluginId;
    }
}

void TimelineController::removeAudioPlugin(int clipId, int index) {
    m_mediaManager->audioMixer()->getChain(clipId).remove(index);
    emit clipEffectsChanged(clipId);
}

void TimelineController::setAudioPluginEnabled(int clipId, int index, bool enabled) {
    if (m_timeline != nullptr) {
        m_timeline->setAudioPluginEnabled(clipId, index, enabled);
    }
}

void TimelineController::reorderAudioPlugins(int clipId, int oldIndex, int newIndex) {
    if (m_timeline != nullptr) {
        m_timeline->reorderAudioPlugins(clipId, oldIndex, newIndex);
    }
}

auto TimelineController::isAudioClip(int clipId) const -> bool {
    const auto *clip = m_timeline->findClipById(clipId);
    return (clip != nullptr) && clip->type == QLatin1String("audio");
}

auto TimelineController::getWaveformPeaks(int clipId, int pixelWidth, int displayDurationFrames) const -> QVariantList { // NOLINT(bugprone-easily-swappable-parameters)
    if (pixelWidth <= 0 || displayDurationFrames <= 0) {
        return {};
    }

    const auto *clip = m_timeline->findClipById(clipId);
    if ((clip == nullptr) || clip->type != "audio") {
        return {};
    }

    auto *decoder = qobject_cast<AviQtl::Core::AudioDecoder *>((m_mediaManager != nullptr) ? m_mediaManager->decoderForClip(clipId) : nullptr);
    if ((decoder == nullptr) || !decoder->isReady()) {
        return QVariantList(pixelWidth, 0.0);
    }

    int fps = static_cast<int>(m_project->fps());
    if (fps <= 0) {
        fps = 60;
    }
    // 渡された displayDurationFrames で秒数を計算 (ドラフト値が来たらそれを使う)
    double displaySec = static_cast<double>(displayDurationFrames) / fps;

    std::vector<float> rawPeaks = decoder->getPeaks(0.0, displaySec, pixelWidth);
    QVariantList result;
    result.reserve(static_cast<qsizetype>(rawPeaks.size()));
    for (float p : rawPeaks) {
        result.append(static_cast<double>(p));
    }

    return result;
}

auto TimelineController::getClipEffectStack(int clipId) const -> QVariantList {
    QVariantList list;
    if (clipId < 0) {
        return list;
    }

    auto &chain = m_mediaManager->audioMixer()->getChain(clipId);
    for (int i = 0; i < chain.count(); ++i) {
        auto *plugin = chain.get(i);
        if (plugin != nullptr) {
            QVariantMap effectInfo;
            effectInfo.insert(QStringLiteral("name"), plugin->name());
            effectInfo.insert(QStringLiteral("format"), plugin->format());
            list.append(effectInfo);
        }
    }
    return list;
}

auto TimelineController::getEffectParameters(int clipId, int effectIndex) const -> QVariantList {
    QVariantList list;
    if (clipId < 0) {
        return list;
    }
    auto &chain = m_mediaManager->audioMixer()->getChain(clipId);
    auto *plugin = chain.get(effectIndex);
    if (plugin != nullptr) {
        for (int i = 0; i < plugin->paramCount(); ++i) {
            QVariantMap paramInfo;
            auto info = plugin->getParamInfo(i);

            paramInfo.insert(QStringLiteral("pIdx"), i);
            paramInfo.insert(QStringLiteral("name"), info.name);
            paramInfo.insert(QStringLiteral("current"), plugin->getParam(i));
            paramInfo.insert(QStringLiteral("min"), info.min);
            paramInfo.insert(QStringLiteral("max"), info.max);

            if (info.isToggle) {
                paramInfo.insert(QStringLiteral("type"), "bool");
            } else if (info.isInteger) {
                paramInfo.insert(QStringLiteral("type"), "int");
            } else {
                paramInfo.insert(QStringLiteral("type"), "slider");
            }

            list.append(paramInfo);
        }
    }
    return list;
}

void TimelineController::setEffectParameter(int clipId, int effectIndex, int paramIndex, float value) {
    if (clipId < 0) {
        return;
    }
    auto &chain = m_mediaManager->audioMixer()->getChain(clipId);
    auto *plugin = chain.get(effectIndex); // NOLINT(bugprone-easily-swappable-parameters)
    if (plugin != nullptr) {
        plugin->setParam(paramIndex, value);
    }
}

void TimelineController::setKeyframe(int clipId, int effectIndex, const QString &paramName, int frame, const QVariant &value, const QVariantMap &options) { m_timeline->setKeyframe(clipId, effectIndex, paramName, frame, value, options); }

void TimelineController::removeKeyframe(int clipId, int effectIndex, const QString &paramName, int frame) { m_timeline->removeKeyframe(clipId, effectIndex, paramName, frame); }

void TimelineController::deleteClip(int clipId) { requestDelete(clipId); }

void TimelineController::requestDelete(int targetClipId) {
    if ((m_timeline == nullptr) || (m_selection == nullptr)) {
        return;
    }

    QVariantList selected = m_selection->selectedClipIds();

    // 選択が1件以上ある場合
    if (!selected.isEmpty()) {
        // 全体削除（Delキーなど）または、選択対象を右クリックして削除する場合
        if (targetClipId < 0 || selected.contains(targetClipId)) {
            m_timeline->deleteClipsByIds(selected);
            return;
        }
    }

    // 選択されていない対象を直接削除しようとする場合
    if (targetClipId >= 0) {
        QVariantList ids{targetClipId};
        m_timeline->applySelectionIds(ids); // 内部的に選択状態を同期
        m_timeline->deleteClipsByIds(ids);
    }
}

void TimelineController::splitClip(int clipId, int frame) {
    if (m_timeline != nullptr) {
        m_timeline->splitClip(clipId, frame);
    }
}

void TimelineController::splitSelectedClips(int frame) {
    if (m_timeline != nullptr) {
        m_timeline->splitSelectedClips(frame);
    }
}

#include "engine/timeline/ecs.hpp"

auto TimelineController::evaluateClipParams(int clipId, int relFrame) const -> QVariantMap {
    QVariantMap out;
    const auto *clip = m_timeline->findClipById(clipId);
    if (clip == nullptr) {
        return out;
    }

    const double fps = project() ? project()->fps() : 60.0;

    for (auto *eff : clip->effects) {
        // 各エフェクトの評価済みパラメータを取得
        QVariantMap p = eff->evaluatedParams(relFrame, fps);
        // 1. エフェクトIDをキーにしてネスト状態で保持（高度なアクセス用）
        out.insert(eff->id(), p);
        // 2. トップレベルにフラットにマージ（BaseObjectや既存のQMLコンポーネント用）
        for (auto it = p.begin(); it != p.end(); ++it) {
            out.insert(it.key(), it.value());
        }
    }
    return out;
}
void TimelineController::copyClip(int clipId) { m_timeline->copyClip(clipId); }

void TimelineController::cutClip(int clipId) { m_timeline->cutClip(clipId); }

void TimelineController::pasteClip(int frame, int layer) { m_timeline->pasteClip(frame, layer); }

void TimelineController::copySelectedClips() {
    if (m_timeline != nullptr) {
        m_timeline->copySelectedClips();
    }
}

void TimelineController::cutSelectedClips() {
    if (m_timeline != nullptr) {
        m_timeline->cutSelectedClips();
    }
}

void TimelineController::deleteSelectedClips() { requestDelete(-1); }

} // namespace AviQtl::UI