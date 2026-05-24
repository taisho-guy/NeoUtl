#include "commands.hpp"
#include "effect_registry.hpp"
#include "selection_service.hpp"
#include "settings_manager.hpp"
#include "timeline_service.hpp"
#include <QDebug>
#include <QPoint>
#include <algorithm>
#include <utility>

namespace AviQtl::UI {

void TimelineService::createClip(const QString &type, int startFrame, int layer) {
    int id = m_nextClipId++;
    QString clipName = type;
    auto meta = AviQtl::Core::EffectRegistry::instance().getEffect(type);
    if (!meta.name.isEmpty()) {
        clipName = meta.name;
    }
    m_undoStack->push(new AddClipCommand(this, id, type, startFrame, layer, clipName));
}

void TimelineService::createClipInternal(int clipId, const QString &type, int startFrame, int layer, bool emitSignal) {
    startFrame = std::max(startFrame, 0);
    layer = std::max(layer, 0);

    if (isLayerLocked(layer)) {
        qWarning() << "createClipInternal: レイヤー" << layer << "はロックされています。";
        return;
    }

    const int defaultDuration = AviQtl::Core::SettingsManager::instance().settings().value(QStringLiteral("defaultClipDuration"), 100).toInt();
    auto overlaps = [](int s1, int d1, int s2, int d2) -> bool { return (s1 < (s2 + d2)) && (s2 < (s1 + d1)); };
    auto &currentClips = clipsMutable();
    for (const auto &c : std::as_const(currentClips)) {
        if (c.layer == layer && overlaps(startFrame, defaultDuration, c.startFrame, c.durationFrames)) {
            qWarning() << "クリップ作成を拒否: レイヤー" << layer << "の" << startFrame << "フレームで衝突が発生";
            return;
        }
    }

    ClipData newClip;
    newClip.id = clipId;
    newClip.sceneId = m_currentSceneId;
    newClip.type = type;
    newClip.startFrame = startFrame;
    newClip.durationFrames = defaultDuration;
    newClip.layer = layer;

    currentClips.append(newClip);

    addEffectInternal(clipId, QStringLiteral("transform"));
    addEffectInternal(clipId, type);
    if (type == QLatin1String("scene")) {
        int defaultTargetSceneId = -1;
        for (const auto &scene : std::as_const(m_scenes)) {
            if (scene.id != m_currentSceneId) {
                defaultTargetSceneId = scene.id;
                break;
            }
        }

        auto *clip = findClipById(clipId);
        if (clip != nullptr) {
            for (auto *eff : std::as_const(clip->effects)) {
                if (eff != nullptr && eff->id() == QLatin1String("scene")) {
                    eff->setParam(QStringLiteral("targetSceneId"), defaultTargetSceneId);
                    break;
                }
            }
        }
    }

    if (emitSignal) {
        emit clipsChanged();
        emit clipCreated(newClip.id, newClip.layer, newClip.startFrame, newClip.durationFrames, newClip.type);
    }
}

void TimelineService::addClipsDirectInternal(const QList<ClipData> &clips) {
    for (const auto &clip : std::as_const(clips)) {
        addClipDirectInternal(clip, false);
    }
    emit clipsChanged();
}

void TimelineService::updateClip(int id, int layer, int startFrame, int duration) {
    const auto *clip = findClipById(id);
    if (clip == nullptr) {
        return;
    }

    QString clipName = clip->type;
    if (!clip->effects.isEmpty()) {
        clipName = clip->effects.first()->name();
    }

    m_undoStack->push(new MoveClipCommand(this, id, clip->layer, clip->startFrame, clip->durationFrames, layer, startFrame, duration, clipName));
}

void TimelineService::insertLayers(int targetLayer, int count, bool above) {
    if (count <= 0)
        return;

    m_undoStack->beginMacro(above ? tr("レイヤーを上に挿入") : tr("レイヤーを下に挿入"));

    QList<ClipData> sceneClips = clips();

    // レイヤー番号が大きい順にソート（下方向にずらすので、下のものから先に動かせば移動先に既存クリップがいない状態を作れる）
    std::sort(sceneClips.begin(), sceneClips.end(), [](const ClipData &a, const ClipData &b) { return a.layer > b.layer; });

    for (const auto &clip : sceneClips) {
        // 「上に挿入」はターゲットレイヤー自体を含む以降をシフト、「下に挿入」はターゲットより下をシフト
        bool shouldShift = above ? (clip.layer >= targetLayer) : (clip.layer > targetLayer);

        if (shouldShift) {
            updateClip(clip.id, clip.layer + count, clip.startFrame, clip.durationFrames);
        }
    }

    m_undoStack->endMacro();
}

void TimelineService::shiftLayers(int startLayer, int endLayer, int delta) {
    if (delta == 0 || startLayer > endLayer)
        return;

    m_undoStack->beginMacro(delta > 0 ? tr("レイヤーをまとめて下へ移動") : tr("レイヤーをまとめて上へ移動"));

    // 現在のシーンのクリップのコピーを取得
    QList<ClipData> sceneClips = clips(); // 処理中のリスト変更を避けるためコピー

    if (delta > 0) {
        // 下方向への移動：下のレイヤーから順に処理
        std::sort(sceneClips.begin(), sceneClips.end(), [](const ClipData &a, const ClipData &b) { return a.layer > b.layer; });
    } else {
        // 上方向への移動：上のレイヤーから順に処理
        std::sort(sceneClips.begin(), sceneClips.end(), [](const ClipData &a, const ClipData &b) { return a.layer < b.layer; });
    }

    for (const auto &clip : sceneClips) {
        // 指定された範囲内のレイヤーに属するクリップのみを対象とする
        if (clip.layer >= startLayer && clip.layer <= endLayer) {
            int newLayer = std::max(0, clip.layer + delta);
            updateClip(clip.id, newLayer, clip.startFrame, clip.durationFrames);
        }
    }

    m_undoStack->endMacro();
}

void TimelineService::applyClipBatchMove(const QVariantList &moves) {
    if (moves.isEmpty()) {
        return;
    }

    m_batchExcludes.clear();
    for (const QVariant &vMove : std::as_const(moves)) {
        m_batchExcludes.insert(vMove.toMap().value(QStringLiteral("id")).toInt());
    }

    struct PendingOp {
        int id;
        int oldLayer;
        int oldStart;
        int targetLayer;
        int targetStart;
        int duration;
        QString name;
    };

    QVector<PendingOp> pending;
    pending.reserve(moves.size());

    for (const QVariant &vMove : std::as_const(moves)) {
        QVariantMap move = vMove.toMap();
        int id = move.value(QStringLiteral("id")).toInt();
        const auto *clip = findClipById(id);
        if (clip != nullptr) {
            pending.push_back(PendingOp{.id = id,
                                        .oldLayer = clip->layer,
                                        .oldStart = clip->startFrame,
                                        .targetLayer = move.value(QStringLiteral("layer")).toInt(),
                                        .targetStart = move.value(QStringLiteral("startFrame")).toInt(),
                                        .duration = move.value(QStringLiteral("duration")).toInt(),
                                        .name = clip->effects.isEmpty() ? clip->type : clip->effects.first()->name()});
        }
    }

    int maxPush = 0;
    bool needsPush = true;
    int loopCount = 0;

    while (needsPush && loopCount < 100) {
        needsPush = false;
        int currentPush = 0;

        for (const auto &op : std::as_const(pending)) {
            int testStart = op.targetStart + maxPush;
            int safeStart = findVacantFrame(op.targetLayer, testStart, op.duration, op.id);
            if (safeStart > testStart) {
                int push = safeStart - testStart;
                currentPush = std::max(push, currentPush);
            }
        }

        if (currentPush > 0) {
            maxPush += currentPush;
            needsPush = true;
        }
        loopCount++;
    }

    m_undoStack->beginMacro(QObject::tr("複数クリップ絶対移動: %1").arg(pending.size()));
    for (const auto &op : std::as_const(pending)) {
        int finalStart = op.targetStart + maxPush;
        m_undoStack->push(new MoveClipCommand(this, op.id, op.oldLayer, op.oldStart, op.duration, op.targetLayer, finalStart, op.duration, op.name));
    }
    m_undoStack->endMacro();
    m_batchExcludes.clear();
}

void TimelineService::moveSelectedClips(int deltaLayer, int deltaFrame) {
    if ((m_selection == nullptr) || (deltaLayer == 0 && deltaFrame == 0)) {
        return;
    }

    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    struct PendingOp {
        int id;
        int oldLayer;
        int oldStart;
        int duration;
        QString name;
    };

    QVector<PendingOp> pending;
    pending.reserve(ids.size());

    for (const QVariant &value : std::as_const(ids)) {
        const int id = value.toInt();
        const auto *clip = findClipById(id);
        if (clip == nullptr) {
            continue;
        }

        pending.push_back(PendingOp{.id = id, .oldLayer = clip->layer, .oldStart = clip->startFrame, .duration = clip->durationFrames, .name = clip->effects.isEmpty() ? clip->type : clip->effects.first()->name()});
    }

    if (deltaFrame > 0 || (deltaFrame == 0 && deltaLayer > 0)) {
        std::ranges::sort(pending, [](const PendingOp &a, const PendingOp &b) -> bool {
            if (a.oldStart != b.oldStart) {
                return a.oldStart > b.oldStart;
            }
            return a.oldLayer > b.oldLayer;
        });
    } else {
        std::ranges::sort(pending, [](const PendingOp &a, const PendingOp &b) -> bool {
            if (a.oldStart != b.oldStart) {
                return a.oldStart < b.oldStart;
            }
            return a.oldLayer < b.oldLayer;
        });
    }

    m_undoStack->beginMacro(QObject::tr("複数クリップ移動: %1").arg(pending.size()));
    for (const PendingOp &clip : std::as_const(pending)) {
        const int newLayer = std::max(0, clip.oldLayer + deltaLayer);
        const int newStart = std::max(0, clip.oldStart + deltaFrame);
        m_undoStack->push(new MoveClipCommand(this, clip.id, clip.oldLayer, clip.oldStart, clip.duration, newLayer, newStart, clip.duration, clip.name));
    }
    m_undoStack->endMacro();
}

void TimelineService::resizeSelectedClips(int deltaStartFrame, int deltaDuration) {
    if ((m_selection == nullptr) || (deltaStartFrame == 0 && deltaDuration == 0)) {
        return;
    }

    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    struct PendingOp {
        int id;
        int oldLayer;
        int oldStart;
        int duration;
        QString name;
    };

    QVector<PendingOp> pending;
    pending.reserve(ids.size());

    for (const QVariant &value : std::as_const(ids)) {
        const int id = value.toInt();
        const auto *clip = findClipById(id);
        if (clip == nullptr) {
            continue;
        }

        pending.push_back(PendingOp{.id = id, .oldLayer = clip->layer, .oldStart = clip->startFrame, .duration = clip->durationFrames, .name = clip->effects.isEmpty() ? clip->type : clip->effects.first()->name()});
    }

    // Resize left side -> deltaStartFrame != 0. If deltaStartFrame > 0, left edge moves right.
    // Resize right side -> deltaStartFrame == 0, deltaDuration != 0.
    // In any case, order matters if they push each other.
    if (deltaStartFrame > 0 || deltaDuration > 0) {
        std::ranges::sort(pending, [](const PendingOp &a, const PendingOp &b) -> bool {
            if (a.oldStart != b.oldStart) {
                return a.oldStart > b.oldStart;
            }
            return a.oldLayer > b.oldLayer;
        });
    } else {
        std::ranges::sort(pending, [](const PendingOp &a, const PendingOp &b) -> bool {
            if (a.oldStart != b.oldStart) {
                return a.oldStart < b.oldStart;
            }
            return a.oldLayer < b.oldLayer;
        });
    }

    m_undoStack->beginMacro(QObject::tr("複数クリップ変形: %1").arg(pending.size()));
    for (const PendingOp &clip : std::as_const(pending)) {
        const int newStart = std::max(0, clip.oldStart + deltaStartFrame);
        const int newDuration = std::max(1, clip.duration + deltaDuration);
        m_undoStack->push(new MoveClipCommand(this, clip.id, clip.oldLayer, clip.oldStart, clip.duration, clip.oldLayer, newStart, newDuration, clip.name));
    }
    m_undoStack->endMacro();
}

auto TimelineService::resolveDragPosition(int clipId, int targetLayer, int proposedStartFrame, const QVariantList &batchIds) -> QPoint { // NOLINT(bugprone-easily-swappable-parameters)
    const auto *movingClip = findClipById(clipId);
    if (movingClip == nullptr) {
        return {proposedStartFrame, targetLayer};
    }

    int deltaLayer = targetLayer - movingClip->layer;
    int deltaFrame = proposedStartFrame - movingClip->startFrame;

    QSet<int> movingIds;
    if (!batchIds.isEmpty()) {
        for (const QVariant &v : std::as_const(batchIds)) {
            movingIds.insert(v.toInt());
        }
    } else if ((m_selection != nullptr) && m_selection->isSelected(clipId)) {
        for (const QVariant &v : m_selection->selectedClipIds()) {
            movingIds.insert(v.toInt());
        }
    } else {
        movingIds.insert(clipId);
    }

    int maxPush = 0;
    bool needsPush = true;
    int loopCount = 0;

    QSet<int> backupExcludes = m_batchExcludes;

    while (needsPush && loopCount < 100) {
        needsPush = false;
        int currentPush = 0;

        for (int id : std::as_const(movingIds)) {
            const auto *c = findClipById(id);
            if (c == nullptr) {
                continue;
            }

            int tLayer = c->layer + deltaLayer;
            tLayer = std::max(tLayer, 0);
            tLayer = std::min(tLayer, 127);

            // ターゲットレイヤーがロックされている場合は、そのクリップの移動を制限
            if (isLayerLocked(tLayer) || isLayerLocked(c->layer)) {
                tLayer = c->layer;
            }

            int testStart = c->startFrame + deltaFrame + maxPush;
            testStart = std::max(testStart, 0);

            m_batchExcludes = movingIds;
            int safeStart = findVacantFrame(tLayer, testStart, c->durationFrames, id);
            m_batchExcludes = backupExcludes;

            if (safeStart > testStart) {
                int push = safeStart - testStart;
                currentPush = std::max(push, currentPush);
            }
        }

        if (currentPush > 0) {
            maxPush += currentPush;
            needsPush = true;
        }
        loopCount++;
    }

    int finalFrame = proposedStartFrame + maxPush;
    finalFrame = std::max(finalFrame, 0);

    int finalLayer = targetLayer;
    finalLayer = std::max(finalLayer, 0);
    finalLayer = std::min(finalLayer, 127);

    return {finalFrame, finalLayer};
}

void TimelineService::updateClipInternal(int id, int layer, int startFrame, int duration, bool emitSignal) {
    const auto *existingClip = findClipById(id);
    if (existingClip == nullptr) {
        return;
    }

    // 移動元または移動先がロックされている場合は拒否
    if (isLayerLocked(layer) || isLayerLocked(existingClip->layer)) {
        qWarning() << "updateClipInternal: ロックされたレイヤーへの/からの操作を拒否しました。";
        return;
    }

    startFrame = std::max(startFrame, 0);
    duration = std::max(duration, 1);
    layer = std::max(layer, 0);

    // [FINAL LOGIC] The ultimate gatekeeper for collision.
    // All position updates, whether from drag, undo, or other operations, must pass this check.
    int safeStartFrame = findVacantFrame(layer, startFrame, duration, id);
    if (safeStartFrame != startFrame) {
        qWarning() << "updateClipInternal: Collision detected. Position adjusted from" << startFrame << "to" << safeStartFrame;
        startFrame = safeStartFrame;
    }

    for (auto &clip : clipsMutable()) {
        if (clip.id == id) {
            if (clip.layer != layer || clip.startFrame != startFrame || clip.durationFrames != duration) {
                clip.layer = layer;
                clip.startFrame = startFrame;
                clip.durationFrames = duration;
                for (auto *effect : std::as_const(clip.effects)) {
                    if (effect != nullptr) {
                        effect->syncTrackEndpoints(duration);
                    }
                }
                if (emitSignal) {
                    emit clipsChanged();
                }
                // 選択中のクリップであればSelectionServiceのキャッシュも更新する
                if (m_selection->selectedClipId() == id) {
                    QVariantMap data = m_selection->selectedClipData();
                    data.insert(QStringLiteral("layer"), layer);
                    data.insert(QStringLiteral("startFrame"), startFrame);
                    data.insert(QStringLiteral("durationFrames"), duration);
                    m_selection->refreshSelectionData(id, data);
                }
            }
            break;
        }
    }
}

void TimelineService::selectClip(int id) { applySelectionIds(QVariantList{id}); }

void TimelineService::toggleSelection(int id, const QVariantMap &data) {
    if (m_selection == nullptr) {
        return;
    }

    QVariantList ids = m_selection->selectedClipIds();
    int idx = -1;
    for (int i = 0; i < ids.size(); ++i) {
        if (ids.value(i).toInt() == id) {
            idx = i;
            break;
        }
    }

    if (idx >= 0) {
        ids.removeAt(idx);
    } else {
        ids.prepend(id);
    }

    applySelectionIds(ids);
}

void TimelineService::applySelectionIds(const QVariantList &ids) {
    int primaryId = -1;
    QVariantMap primaryData;

    // 選択されたクリップのリストを更新
    QVariantList newSelectedIds;
    for (const QVariant &v : std::as_const(ids)) {
        if (!newSelectedIds.contains(v)) {
            newSelectedIds.append(v);
        }
    }

    if (!newSelectedIds.isEmpty()) {
        int id = newSelectedIds.first().toInt(); // 最初のクリップをプライマリとする
        const auto *clip = findClipById(id);
        if (clip != nullptr) { // findClipById は nullptr を返す可能性があるのでチェック
            primaryId = clip->id;
            for (auto *eff : clip->effects) {
                QVariantMap params = eff->params();
                for (auto it = params.begin(); it != params.end(); ++it) {
                    primaryData.insert(it.key(), it.value());
                }
            }
            primaryData.insert(QStringLiteral("startFrame"), clip->startFrame);
            primaryData.insert(QStringLiteral("durationFrames"), clip->durationFrames);
            primaryData.insert(QStringLiteral("layer"), clip->layer);
            primaryData.insert(QStringLiteral("type"), clip->type);
        }
    }

    // SelectionService の replaceSelection を呼び出す
    m_selection->replaceSelection(newSelectedIds, primaryId, primaryData);
}

void TimelineService::selectClipsInRange(int frameA, int frameB, int layerA, int layerB, bool additive) { // NOLINT(bugprone-easily-swappable-parameters)
    const int minFrame = std::min(frameA, frameB);
    const int maxFrame = std::max(frameA, frameB);
    const int minLayer = std::min(layerA, layerB);
    const int maxLayer = std::max(layerA, layerB);

    QVariantList ids;
    int primaryId = -1;
    QVariantMap primaryData;

    for (const auto &clip : clips()) {
        const int clipStart = clip.startFrame;
        const int clipEnd = clip.startFrame + clip.durationFrames;
        const bool frameOverlap = clipStart < maxFrame && minFrame < clipEnd;
        const bool layerMatch = clip.layer >= minLayer && clip.layer <= maxLayer;
        if (!frameOverlap || !layerMatch) {
            continue;
        }

        ids.append(clip.id);

        if (primaryId == -1) {
            primaryId = clip.id;
            for (auto *eff : std::as_const(clip.effects)) {
                QVariantMap params = eff->params();
                for (auto it = params.begin(); it != params.end(); ++it) {
                    primaryData.insert(it.key(), it.value());
                }
            }
            primaryData.insert(QStringLiteral("startFrame"), clip.startFrame);
            primaryData.insert(QStringLiteral("durationFrames"), clip.durationFrames);
            primaryData.insert(QStringLiteral("layer"), clip.layer);
            primaryData.insert(QStringLiteral("type"), clip.type);
        }
    }

    // 既存の選択とマージ
    if (additive) {
        QVariantList merged = m_selection->selectedClipIds();
        for (const QVariant &id : std::as_const(ids)) {
            if (!merged.contains(id)) {
                merged.append(id);
            }
        }
        // プライマリIDがまだ設定されていなければ、マージ後の最初のクリップをプライマリにする
        if (primaryId == -1 && !merged.isEmpty()) {
            primaryId = merged.first().toInt();
        }
        m_selection->replaceSelection(merged, primaryId, primaryData);
        return;
    }
    m_selection->replaceSelection(ids, primaryId, primaryData);
}

void TimelineService::deleteSelectedClips() {
    if (m_selection == nullptr) {
        return;
    }
    deleteClipsByIds(m_selection->selectedClipIds());
}

void TimelineService::deleteClipsByIds(const QVariantList &ids) {
    if (ids.isEmpty()) {
        return;
    }

    QList<int> intIds;
    for (const QVariant &v : std::as_const(ids)) {
        int id = v.toInt();
        if (id >= 0) {
            intIds.append(id);
        }
    }

    if (intIds.isEmpty()) {
        return;
    }

    QString macroText = intIds.size() == 1 ? QObject::tr("クリップ削除") : QObject::tr("複数クリップ削除: %1").arg(intIds.size());

    m_undoStack->push(new DeleteClipsCommand(this, intIds, macroText));

    m_selection->clearSelection();
}

void TimelineService::deleteClip(int clipId) { deleteClipsByIds({clipId}); }

void TimelineService::deleteClipInternal(int clipId, bool emitSignal) {
    auto &currentClips = clipsMutable();
    auto it = std::ranges::find_if(currentClips, [clipId](const ClipData &c) -> bool { return c.id == clipId; });
    if (it != currentClips.end()) {
        for (auto *eff : it->effects) {
            eff->deleteLater();
        }
        currentClips.erase(it);
        if (emitSignal) {
            emit clipsChanged();
        }
    }
}

void TimelineService::addClipDirectInternal(const ClipData &clip, bool emitSignal) {
    clipsMutable().append(clip);
    if (emitSignal) {
        emit clipsChanged();
        emit clipCreated(clip.id, clip.layer, clip.startFrame, clip.durationFrames, clip.type);
    }
}

auto TimelineService::findClipById(int clipId) -> ClipData * {
    for (auto &scene : m_scenes) {
        auto it = std::ranges::find_if(scene.clips, [clipId](const ClipData &c) -> bool { return c.id == clipId; });
        if (it != scene.clips.end())
            return &(*it);
    }
    return nullptr;
}

auto TimelineService::findClipById(int clipId) const -> const ClipData * {
    for (const auto &scene : std::as_const(m_scenes)) {
        auto it = std::ranges::find_if(scene.clips, [clipId](const ClipData &c) -> bool { return c.id == clipId; });
        if (it != scene.clips.end())
            return &(*it);
    }
    return nullptr;
}

auto TimelineService::deepCopyClip(const ClipData &source) -> ClipData {
    ClipData newClip;
    newClip.id = -1;
    newClip.type = source.type;
    newClip.startFrame = source.startFrame;
    newClip.durationFrames = source.durationFrames;
    newClip.layer = source.layer;

    for (const auto *oldEffect : std::as_const(source.effects)) {
        auto *newEffect = new EffectModel(oldEffect->id(), oldEffect->name(), oldEffect->kind(), oldEffect->categories(), oldEffect->params(), oldEffect->qmlSource(), oldEffect->uiDefinition(), this);
        newEffect->setEnabled(oldEffect->isEnabled());
        newEffect->setKeyframeTracks(oldEffect->keyframeTracks());
        newEffect->syncTrackEndpoints(source.durationFrames);
        newClip.effects.append(newEffect);
    }
    return newClip;
}

void TimelineService::copyClip(int clipId) {
    auto &currentClips = clipsMutable();
    auto it = std::ranges::find_if(currentClips, [clipId](const ClipData &c) -> bool { return c.id == clipId; });
    if (it == currentClips.end()) {
        return;
    }

    setClipboard(*it);
}

void TimelineService::copySelectedClips() {
    QList<ClipData> copied;
    const QVariantList ids = m_selection->selectedClipIds();
    for (const QVariant &value : std::as_const(ids)) {
        const int id = value.toInt();
        const auto *clip = findClipById(id);
        if (clip != nullptr) {
            copied.append(deepCopyClip(*clip));
        }
    }
    if (!copied.isEmpty()) {
        setClipboard(copied);
    }
}

void TimelineService::cutClip(int clipId) {
    const auto *clip = findClipById(clipId); // findClipById は const なので、ここでコピー
    if (clip == nullptr) {
        return;
    }
    QString name = clip->effects.isEmpty() ? clip->type : clip->effects.first()->name();
    m_undoStack->push(new CutClipCommand(this, clipId, name));
}

void TimelineService::cutSelectedClips() {
    if ((m_selection == nullptr) || m_selection->selectedClipIds().isEmpty()) {
        return;
    }

    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    QList<ClipData> copied;
    for (const QVariant &v : std::as_const(ids)) {
        const auto *clip = findClipById(v.toInt());
        if (clip != nullptr) {
            copied.append(deepCopyClip(*clip));
        }
    }
    setClipboard(copied); // クリップボードにコピー

    QList<int> intIds;
    for (const QVariant &v : std::as_const(ids)) {
        intIds.append(v.toInt());
    }
    m_undoStack->push(new DeleteClipsCommand(this, intIds, QString(QStringLiteral("複数クリップ切り取り: %1")).arg(ids.size())));
    m_selection->clearSelection();
}

void TimelineService::splitSelectedClips(int frame) {
    if (m_selection == nullptr) {
        return;
    }
    const QVariantList ids = m_selection->selectedClipIds();
    if (ids.isEmpty()) {
        return;
    }

    m_undoStack->beginMacro(QObject::tr("複数クリップ分割: %1").arg(ids.size()));
    for (const QVariant &v : std::as_const(ids)) {
        splitClip(v.toInt(), frame);
    }
    m_undoStack->endMacro();
}

void TimelineService::pasteClip(int frame, int layer) {
    if (m_clipboard.isEmpty()) {
        return;
    }

    frame = std::max(frame, 0);
    layer = std::max(layer, 0);

    auto overlaps = [](int s1, int d1, int s2, int d2) -> bool { return (s1 < (s2 + d2)) && (s2 < (s1 + d1)); };
    auto &currentClips = clipsMutable();

    int baseFrame = m_clipboard.first().startFrame;
    int baseLayer = m_clipboard.first().layer;
    for (const auto &clip : std::as_const(m_clipboard)) {
        baseFrame = std::min(baseFrame, clip.startFrame);
        baseLayer = std::min(baseLayer, clip.layer);
    }

    QList<ClipData> pending;
    for (const auto &src : std::as_const(m_clipboard)) {
        ClipData newClip = deepCopyClip(src);
        newClip.startFrame = frame + (src.startFrame - baseFrame);
        newClip.layer = std::max(0, layer + (src.layer - baseLayer));

        for (const auto &c : std::as_const(currentClips)) {
            if (c.layer == newClip.layer && overlaps(newClip.startFrame, newClip.durationFrames, c.startFrame, c.durationFrames)) {
                qWarning() << "クリップ貼り付けを拒否: レイヤー" << newClip.layer << "の" << newClip.startFrame << "フレームで衝突が発生";
                return;
            }
        }
        for (const auto &c : std::as_const(pending)) {
            if (c.layer == newClip.layer && overlaps(newClip.startFrame, newClip.durationFrames, c.startFrame, c.durationFrames)) {
                qWarning() << "クリップ貼り付けを拒否: 貼り付け対象同士が衝突";
                return;
            }
        }

        pending.append(newClip);
    }

    if (pending.size() == 1) {
        int newId = m_nextClipId++;
        m_undoStack->push(new PasteClipCommand(this, newId, pending.first()));
        return;
    }

    m_undoStack->beginMacro(QObject::tr("複数クリップ貼り付け: %1").arg(pending.size()));
    for (const auto &clip : std::as_const(pending)) {
        int newId = m_nextClipId++;
        m_undoStack->push(new PasteClipCommand(this, newId, clip));
    }
    m_undoStack->endMacro();
}

void TimelineService::splitClip(int clipId, int frame) {
    const auto *clip = findClipById(clipId);
    if (clip == nullptr) {
        return;
    }

    if (frame > clip->startFrame && frame < clip->startFrame + clip->durationFrames) {
        QString clipName = clip->type;
        if (!clip->effects.isEmpty()) {
            clipName = clip->effects.first()->name();
        }
        m_undoStack->push(new SplitClipCommand(this, clipId, frame, clipName));
    }
}

auto TimelineService::clips() const -> const QList<ClipData> & { return currentScene()->clips; }

auto TimelineService::clipsMutable() -> QList<ClipData> & { return currentScene()->clips; }

auto TimelineService::clips(int sceneId) const -> const QList<ClipData> & {
    for (const auto &scene : std::as_const(m_scenes)) {
        if (scene.id == sceneId) {
            return scene.clips;
        }
    }
    static QList<ClipData> empty;
    return empty;
}

auto TimelineService::findVacantFrame(int layer, int startFrame, int duration, int excludeClipId) const -> int { // NOLINT(bugprone-easily-swappable-parameters)
    QList<const ClipData *> layerClips;

    // バッチ移動中は明示的に指定された集合を使い、そうでない場合は選択情報を使う
    bool isBatchMode = !m_batchExcludes.isEmpty();
    bool isSelected = (m_selection != nullptr) && m_selection->isSelected(excludeClipId);
    QVariantList selectedIds = isSelected ? m_selection->selectedClipIds() : QVariantList();

    for (const auto &clip : clips()) {
        if (clip.id == excludeClipId) {
            continue;
        }

        if (isBatchMode) {
            if (m_batchExcludes.contains(clip.id)) {
                continue;
            }
        } else if (isSelected) {
            bool isPeer = false;
            for (const QVariant &v : std::as_const(selectedIds)) {
                if (v.toInt() == clip.id) {
                    isPeer = true;
                    break;
                }
            }
            if (isPeer) {
                continue;
            }
        }

        if (clip.layer == layer) {
            layerClips.append(&clip);
        }
    }

    std::ranges::sort(layerClips, [](const ClipData *a, const ClipData *b) -> bool { return a->startFrame < b->startFrame; });

    int candidateStart = std::max(0, startFrame);
    for (const auto &clip : std::as_const(layerClips)) {
        int clipEnd = clip->startFrame + clip->durationFrames;
        int candidateEnd = candidateStart + duration;
        if (candidateStart < clipEnd && candidateEnd > clip->startFrame) {
            candidateStart = clipEnd;
        }
    }
    return candidateStart;
}

void TimelineService::setClipboard(const ClipData &clip) { setClipboard(QList<ClipData>{clip}); }

void TimelineService::setClipboard(const QList<ClipData> &clips) {
    // 既存のエフェクトを解放
    for (auto &c : m_clipboard) {
        for (auto *eff : std::as_const(c.effects)) {
            if (eff)
                eff->deleteLater();
        }
        c.effects.clear();
    }
    m_clipboard.clear();

    for (const auto &clip : std::as_const(clips)) {
        m_clipboard.append(deepCopyClip(clip));
    }
}

} // namespace AviQtl::UI
