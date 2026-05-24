#include "commands.hpp"
#include "effect_registry.hpp"
#include "timeline_service.hpp"
#include <QObject>

namespace AviQtl::UI {

AddClipCommand::AddClipCommand(TimelineService *service, int clipId, QString type, int startFrame, int layer, const QString &clipName) // NOLINT(bugprone-easily-swappable-parameters)
    : m_service(service), m_clipId(clipId), m_type(std::move(type)), m_startFrame(startFrame), m_layer(layer), m_clipName(clipName) {
    setText(QObject::tr("クリップ追加: %1").arg(clipName));
}
void AddClipCommand::undo() { m_service->deleteClipInternal(m_clipId); }
void AddClipCommand::redo() { m_service->createClipInternal(m_clipId, m_type, m_startFrame, m_layer); }

MoveClipCommand::MoveClipCommand(TimelineService *service, int clipId, int oldLayer, int oldStart, int oldDuration, int newLayer, int newStart, int newDuration, const QString &clipName) // NOLINT(bugprone-easily-swappable-parameters)
    : m_service(service), m_clipId(clipId), m_oldLayer(oldLayer), m_oldStart(oldStart), m_oldDuration(oldDuration), m_newLayer(newLayer), m_newStart(newStart), m_newDuration(newDuration), m_clipName(clipName) {
    setText(QObject::tr("クリップ移動: %1").arg(clipName));
}
void MoveClipCommand::undo() { m_service->updateClipInternal(m_clipId, m_oldLayer, m_oldStart, m_oldDuration); }
void MoveClipCommand::redo() { m_service->updateClipInternal(m_clipId, m_newLayer, m_newStart, m_newDuration); }

UpdateEffectParamCommand::UpdateEffectParamCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, QVariant newValue, QVariant oldValue, const QString &effectName) // NOLINT(bugprone-easily-swappable-parameters)
    : m_service(service), m_clipId(clipId), m_effectIndex(effectIndex), m_paramName(paramName), m_newValue(std::move(newValue)), m_oldValue(std::move(oldValue)), m_effectName(effectName) {
    setText(QObject::tr("パラメータ変更: %1 - %2").arg(effectName).arg(paramName));
}
void UpdateEffectParamCommand::undo() { m_service->updateEffectParamInternal(m_clipId, m_effectIndex, m_paramName, m_oldValue); }
void UpdateEffectParamCommand::redo() { m_service->updateEffectParamInternal(m_clipId, m_effectIndex, m_paramName, m_newValue); }
auto UpdateEffectParamCommand::id() const -> int { return 1001; } // パラメータ変更コマンドのID
auto UpdateEffectParamCommand::mergeWith(const QUndoCommand *other) -> bool {
    if (other->id() != id()) {
        return false;
    }
    const auto *cmd = dynamic_cast<const UpdateEffectParamCommand *>(other);
    if (cmd->m_clipId != m_clipId || cmd->m_effectIndex != m_effectIndex || cmd->m_paramName != m_paramName) {
        return false;
    }
    m_newValue = cmd->m_newValue; // 連続する同じパラメータの変更はマージする
    redo();                       // モデルを最新値で更新し、シグナルを発火させる
    return true;
}

AddEffectCommand::AddEffectCommand(TimelineService *service, int clipId, QString effectId, const QString &effectName)
    : m_service(service), m_clipId(clipId), m_effectId(std::move(effectId)), m_effectName(effectName) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("エフェクト追加: %1").arg(effectName));
}
void AddEffectCommand::undo() { m_service->removeEffectInternal(m_clipId, -1); }
void AddEffectCommand::redo() { m_service->addEffectInternal(m_clipId, m_effectId); }

RemoveEffectCommand::RemoveEffectCommand(TimelineService *service, int clipId, int effectIndex, const QString &effectName)
    : m_service(service), m_clipId(clipId), m_effectIndex(effectIndex), m_effectName(effectName) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("エフェクト削除: %1").arg(effectName));
}
void RemoveEffectCommand::redo() { m_service->removeEffectInternal(m_clipId, m_effectIndex); }
void RemoveEffectCommand::undo() { m_service->restoreEffectInternal(m_clipId, m_removedEffectData); }

RemoveMultipleEffectsCommand::RemoveMultipleEffectsCommand(TimelineService *service, int clipId, const QList<int> &sortedDescIndices, const QString &macroText) : m_service(service), m_clipId(clipId), m_sortedDescIndices(sortedDescIndices) {
    setText(macroText);
}

void RemoveMultipleEffectsCommand::redo() { m_service->removeMultipleEffectsInternal(m_clipId, m_sortedDescIndices, &m_removedEffectsData); }

void RemoveMultipleEffectsCommand::undo() { m_service->restoreMultipleEffectsInternal(m_clipId, m_removedEffectsData); }

ReorderEffectCommand::ReorderEffectCommand(TimelineService *service, int clipId, int oldIndex, int newIndex) : m_service(service), m_clipId(clipId), m_oldIndex(oldIndex), m_newIndex(newIndex) {
    setText(QObject::tr("エフェクト順序変更"));
} // NOLINT(bugprone-easily-swappable-parameters)
void ReorderEffectCommand::undo() { m_service->reorderEffectsInternal(m_clipId, m_newIndex, m_oldIndex); }
void ReorderEffectCommand::redo() { m_service->reorderEffectsInternal(m_clipId, m_oldIndex, m_newIndex); }

ReorderMultipleEffectsCommand::ReorderMultipleEffectsCommand(TimelineService *service, int clipId, QList<int> redoPerm, QList<int> undoPerm, const QString &text)
    : m_service(service), m_clipId(clipId), m_redoPerm(std::move(redoPerm)), m_undoPerm(std::move(undoPerm)) {
    setText(text);
}
void ReorderMultipleEffectsCommand::undo() { m_service->applyPermutationInternal(m_clipId, m_undoPerm); }
void ReorderMultipleEffectsCommand::redo() { m_service->applyPermutationInternal(m_clipId, m_redoPerm); }

ReorderAudioPluginCommand::ReorderAudioPluginCommand(TimelineService *service, int clipId, int oldIndex, int newIndex) : m_service(service), m_clipId(clipId), m_oldIndex(oldIndex), m_newIndex(newIndex) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("オーディオプラグイン順序変更"));
}
void ReorderAudioPluginCommand::undo() { m_service->reorderAudioPluginsInternal(m_clipId, m_newIndex, m_oldIndex); }
void ReorderAudioPluginCommand::redo() { m_service->reorderAudioPluginsInternal(m_clipId, m_oldIndex, m_newIndex); }

SetEffectEnabledCommand::SetEffectEnabledCommand(TimelineService *service, int clipId, int effectIndex, bool enabled) : m_service(service), m_clipId(clipId), m_effectIndex(effectIndex), m_enabled(enabled) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("エフェクト有効/無効切り替え"));
}
void SetEffectEnabledCommand::undo() { m_service->setEffectEnabledInternal(m_clipId, m_effectIndex, !m_enabled); }
void SetEffectEnabledCommand::redo() { m_service->setEffectEnabledInternal(m_clipId, m_effectIndex, m_enabled); }

SetAudioPluginEnabledCommand::SetAudioPluginEnabledCommand(TimelineService *service, int clipId, int index, bool enabled) : m_service(service), m_clipId(clipId), m_index(index), m_enabled(enabled) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("オーディオプラグイン有効/無効切り替え"));
}
void SetAudioPluginEnabledCommand::undo() { m_service->setAudioPluginEnabledInternal(m_clipId, m_index, !m_enabled); }
void SetAudioPluginEnabledCommand::redo() { m_service->setAudioPluginEnabledInternal(m_clipId, m_index, m_enabled); }

PasteEffectCommand::PasteEffectCommand(TimelineService *service, int clipId, int targetIndex, EffectModel *templateEffect)
    : m_service(service), m_clipId(clipId), m_targetIndex(targetIndex), m_effect(templateEffect->clone()) { // NOLINT(bugprone-easily-swappable-parameters)

    setText(QObject::tr("エフェクト貼り付け"));
}
void PasteEffectCommand::undo() { m_service->removeEffectInternal(m_clipId, m_targetIndex); }
void PasteEffectCommand::redo() { m_service->pasteEffectInternal(m_clipId, m_targetIndex, m_effect); }

UpdateLayerStateCommand::UpdateLayerStateCommand(TimelineService *service, int sceneId, int layer, bool value, StateType type)
    : m_service(service), m_sceneId(sceneId), m_layer(layer), m_value(value), m_type(type) { // NOLINT(bugprone-easily-swappable-parameters)
    QString actionName = (type == Lock) ? (value ? QObject::tr("レイヤーロック") : QObject::tr("ロック解除")) : (value ? QObject::tr("レイヤー非表示") : QObject::tr("レイヤー表示"));
    setText(QObject::tr("%1: レイヤー %2").arg(actionName).arg(m_layer));
}
void UpdateLayerStateCommand::undo() { m_service->setLayerStateInternal(m_sceneId, m_layer, !m_value, m_type); }
void UpdateLayerStateCommand::redo() { m_service->setLayerStateInternal(m_sceneId, m_layer, m_value, m_type); }

SplitClipCommand::SplitClipCommand(TimelineService *service, int clipId, int frame, const QString &clipName)
    : m_service(service), m_originalClipId(clipId), m_newClipId(-1), m_splitFrame(frame), m_originalDuration(0), m_clipName(clipName) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("クリップ分割: %1").arg(clipName));
}

void SplitClipCommand::undo() {
    m_service->deleteClipInternal(m_newClipId);
    // 元のクリップの長さを復元
    const auto &clips = m_service->clips();
    auto it = std::ranges::find_if(clips, [this](const ClipData &c) -> bool { return c.id == m_originalClipId; });
    if (it != clips.end()) {
        m_service->updateClipInternal(m_originalClipId, it->layer, it->startFrame, m_originalDuration);
    }
}

void SplitClipCommand::redo() {
    const auto &clips = m_service->clips();
    auto it = std::ranges::find_if(clips, [this](const ClipData &c) -> bool { return c.id == m_originalClipId; });
    if (it == clips.end()) {
        return;
    }

    // 分割前の状態を保存・計算
    if (m_newClipId == -1) {
        m_newClipId = m_service->nextClipId();
        m_service->setNextClipId(m_newClipId + 1);
        m_originalDuration = it->durationFrames;
    }

    int firstHalfDuration = m_splitFrame - it->startFrame;
    int secondHalfDuration = m_originalDuration - firstHalfDuration;

    // 後半部分のクリップを作成
    ClipData newClip = m_service->deepCopyClip(*it);
    newClip.id = m_newClipId;
    newClip.startFrame = m_splitFrame;
    newClip.durationFrames = secondHalfDuration;

    for (int i = 0; i < it->effects.size() && i < newClip.effects.size(); ++i) {
        auto *originalEffect = it->effects.value(i);
        auto *newEffect = newClip.effects.value(i);
        if ((originalEffect == nullptr) || (newEffect == nullptr)) {
            continue;
        }

        QVariantMap secondHalfTracks = originalEffect->splitTracks(firstHalfDuration, m_originalDuration);
        originalEffect->syncTrackEndpoints(firstHalfDuration);
        newEffect->setKeyframeTracks(secondHalfTracks);
        newEffect->syncTrackEndpoints(secondHalfDuration);
    }

    m_service->updateClipInternal(m_originalClipId, it->layer, it->startFrame, firstHalfDuration);
    m_service->addClipDirectInternal(newClip);
}

DeleteClipsCommand::DeleteClipsCommand(TimelineService *service, const QList<int> &clipIds, const QString &macroText) : m_service(service), m_clipIds(clipIds) {
    setText(macroText);
    for (int id : std::as_const(clipIds)) {
        const auto *clip = service->findClipById(id);
        if (clip != nullptr) {
            ClipData snap = service->deepCopyClip(*clip);
            snap.id = id; // 重要: 削除前の元のIDをスナップショットに保存
            m_snapshots.append(snap);
        }
    }
}
void DeleteClipsCommand::redo() {
    for (int id : std::as_const(m_clipIds)) {
        m_service->deleteClipInternal(id, false);
    }
    emit m_service->clipsChanged();
}
void DeleteClipsCommand::undo() { m_service->addClipsDirectInternal(m_snapshots); }

CutClipCommand::CutClipCommand(TimelineService *service, int clipId, const QString &clipName) : m_service(service), m_clipId(clipId) {
    const auto *clip = service->findClipById(clipId);
    if (clip != nullptr) {
        m_snapshot = service->deepCopyClip(*clip);
    }
    setText(QObject::tr("切り取り: %1").arg(clipName));
}
void CutClipCommand::redo() {
    m_service->setClipboard(m_snapshot);
    m_service->deleteClipInternal(m_clipId);
}
void CutClipCommand::undo() {
    m_snapshot.id = m_clipId;
    m_service->addClipDirectInternal(m_snapshot);
}

PasteClipCommand::PasteClipCommand(TimelineService *service, int newClipId, const ClipData &clipData) : m_service(service), m_newClipId(newClipId), m_clipData(clipData) { setText(QObject::tr("貼り付け: %1").arg(clipData.type)); }
void PasteClipCommand::redo() {
    ClipData c = m_clipData;
    c.id = m_newClipId;
    m_service->addClipDirectInternal(c);
}
void PasteClipCommand::undo() { m_service->deleteClipInternal(m_newClipId); }

SetKeyframeCommand::SetKeyframeCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, int frame, QVariant newValue, QVariantMap options, QVariant oldValue, QVariantMap oldOptions,
                                       bool wasExisting) // NOLINT(bugprone-easily-swappable-parameters)
    : m_service(service), m_clipId(clipId), m_effectIndex(effectIndex), m_frame(frame), m_paramName(paramName), m_newValue(std::move(newValue)), m_oldValue(std::move(oldValue)), m_newOptions(std::move(options)), m_oldOptions(std::move(oldOptions)),
      m_wasExisting(wasExisting) {
    setText(QObject::tr("キーフレーム設定: %1").arg(paramName));
}
void SetKeyframeCommand::redo() { m_service->setKeyframeInternal(m_clipId, m_effectIndex, m_paramName, m_frame, m_newValue, m_newOptions); }
void SetKeyframeCommand::undo() {
    if (m_wasExisting) {
        m_service->setKeyframeInternal(m_clipId, m_effectIndex, m_paramName, m_frame, m_oldValue, m_oldOptions);
    } else {
        m_service->removeKeyframeInternal(m_clipId, m_effectIndex, m_paramName, m_frame);
    }
}
auto SetKeyframeCommand::id() const -> int { return 1002; }
auto SetKeyframeCommand::mergeWith(const QUndoCommand *other) -> bool {
    if (other->id() != id()) {
        return false;
    }
    const auto *cmd = dynamic_cast<const SetKeyframeCommand *>(other);
    if (cmd->m_clipId != m_clipId || cmd->m_effectIndex != m_effectIndex || cmd->m_paramName != m_paramName || cmd->m_frame != m_frame) {
        return false;
    }
    m_newValue = cmd->m_newValue;
    m_newOptions = cmd->m_newOptions;
    redo(); // マージ中もモデルを同期
    return true;
}

RemoveKeyframeCommand::RemoveKeyframeCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, int frame, QVariant savedValue, QVariantMap savedOptions) // NOLINT(bugprone-easily-swappable-parameters)
    : m_service(service), m_clipId(clipId), m_effectIndex(effectIndex), m_frame(frame), m_paramName(paramName), m_savedValue(std::move(savedValue)), m_savedOptions(std::move(savedOptions)) {
    setText(QObject::tr("キーフレーム削除: %1 [%2]").arg(paramName).arg(frame));
}
void RemoveKeyframeCommand::redo() { m_service->removeKeyframeInternal(m_clipId, m_effectIndex, m_paramName, m_frame); }
void RemoveKeyframeCommand::undo() { m_service->setKeyframeInternal(m_clipId, m_effectIndex, m_paramName, m_frame, m_savedValue, m_savedOptions); }

AddSceneCommand::AddSceneCommand(TimelineService *service, int sceneId, const QString &name) : m_service(service), m_sceneId(sceneId), m_name(name) { setText(QObject::tr("シーン追加: %1").arg(name)); }
void AddSceneCommand::redo() { m_service->createSceneInternal(m_sceneId, m_name); }
void AddSceneCommand::undo() { m_service->removeSceneInternal(m_sceneId); }

RemoveSceneCommand::RemoveSceneCommand(TimelineService *service, int sceneId, const QString &name) : m_service(service), m_sceneId(sceneId) {
    for (const auto &s : service->getAllScenes()) {
        if (s.id == sceneId) {
            m_snapshot = s;
            break;
        }
    }
    setText(QObject::tr("シーン削除: %1").arg(name));
}
void RemoveSceneCommand::redo() { m_service->removeSceneInternal(m_sceneId); }
void RemoveSceneCommand::undo() { m_service->restoreSceneInternal(m_snapshot); }

UpdateSceneSettingsCommand::UpdateSceneSettingsCommand(TimelineService *service, int sceneId, SceneData oldData, const SceneData &newData)
    : m_service(service), m_sceneId(sceneId), m_oldData(std::move(oldData)), m_newData(newData) { // NOLINT(bugprone-easily-swappable-parameters)
    setText(QObject::tr("シーン設定変更: %1").arg(newData.name));
}
void UpdateSceneSettingsCommand::redo() { m_service->applySceneSettingsInternal(m_sceneId, m_newData); }
void UpdateSceneSettingsCommand::undo() { m_service->applySceneSettingsInternal(m_sceneId, m_oldData); }

} // namespace AviQtl::UI