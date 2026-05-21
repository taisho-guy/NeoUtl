#pragma once
#include "timeline_service.hpp"
#include <QUndoCommand>

namespace AviQtl::UI {

class AddClipCommand : public QUndoCommand {
  public:
    AddClipCommand(TimelineService *service, int clipId, QString type, int startFrame, int layer, const QString &clipName);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    QString m_type;
    int m_startFrame;
    int m_layer;
    QString m_clipName;
};

class MoveClipCommand : public QUndoCommand {
  public:
    MoveClipCommand(TimelineService *service, int clipId, int oldLayer, int oldStart, int oldDuration, int newLayer, int newStart, int newDuration, const QString &clipName);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    int m_oldLayer, m_oldStart, m_oldDuration;
    int m_newLayer, m_newStart, m_newDuration;
    QString m_clipName;
};

class UpdateEffectParamCommand : public QUndoCommand {
  public:
    UpdateEffectParamCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, QVariant newValue, QVariant oldValue, const QString &effectName);
    void undo() override;
    void redo() override;
    int id() const override;
    bool mergeWith(const QUndoCommand *other) override;

  private:
    TimelineService *m_service;
    int m_clipId;
    int m_effectIndex;
    QString m_paramName;
    QVariant m_newValue;
    QVariant m_oldValue;
    QString m_effectName;
};

class AddEffectCommand : public QUndoCommand {
  public:
    AddEffectCommand(TimelineService *service, int clipId, QString effectId, const QString &effectName);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    QString m_effectId;
    QString m_effectName;
};

class RemoveEffectCommand : public QUndoCommand {
  public:
    RemoveEffectCommand(TimelineService *service, int clipId, int effectIndex, const QString &effectName);
    void undo() override;
    void redo() override;
    void setRemovedEffect(const QVariantMap &effectData) { m_removedEffectData = effectData; }

  private:
    TimelineService *m_service;
    int m_clipId;
    int m_effectIndex;
    QVariantMap m_removedEffectData;
    QString m_effectName;
};

class RemoveMultipleEffectsCommand : public QUndoCommand {
  public:
    RemoveMultipleEffectsCommand(TimelineService *service, int clipId, const QList<int> &sortedDescIndices, const QString &macroText);
    void undo() override;
    void redo() override;
    void setRemovedEffects(const QList<QVariantMap> &data) { m_removedEffectsData = data; }

  private:
    TimelineService *m_service;
    int m_clipId;
    QList<int> m_sortedDescIndices;
    QList<QVariantMap> m_removedEffectsData;
};

class ReorderEffectCommand : public QUndoCommand {
  public:
    ReorderEffectCommand(TimelineService *service, int clipId, int oldIndex, int newIndex);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    int m_oldIndex, m_newIndex;
};

class ReorderMultipleEffectsCommand : public QUndoCommand {
  public:
    // 生ポインタ保持によるダングリング回避: 置換順列インデックスだけ保持する
    ReorderMultipleEffectsCommand(TimelineService *service, int clipId, QList<int> redoPerm, QList<int> undoPerm, const QString &text);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    QList<int> m_redoPerm; // 旧順序→新順序を適用する置換
    QList<int> m_undoPerm; // 新順序→旧順序を復元する置換
};

class ReorderAudioPluginCommand : public QUndoCommand {
  public:
    ReorderAudioPluginCommand(TimelineService *service, int clipId, int oldIndex, int newIndex);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    int m_oldIndex, m_newIndex;
};

class SetEffectEnabledCommand : public QUndoCommand {
  public:
    SetEffectEnabledCommand(TimelineService *service, int clipId, int effectIndex, bool enabled);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId, m_effectIndex;
    bool m_enabled;
};

class PasteEffectCommand : public QUndoCommand {
  public:
    PasteEffectCommand(TimelineService *service, int clipId, int targetIndex, EffectModel *templateEffect);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId, m_targetIndex;
    EffectModel *m_effect;
};

class SetAudioPluginEnabledCommand : public QUndoCommand {
  public:
    SetAudioPluginEnabledCommand(TimelineService *service, int clipId, int index, bool enabled);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId, m_index;
    bool m_enabled;
};

class SplitClipCommand : public QUndoCommand {
  public:
    SplitClipCommand(TimelineService *service, int clipId, int frame, const QString &clipName);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_originalClipId;
    int m_newClipId;
    int m_splitFrame;
    int m_originalDuration;
    QString m_clipName;
};

class DeleteClipsCommand : public QUndoCommand {
  public:
    DeleteClipsCommand(TimelineService *service, const QList<int> &clipIds, const QString &macroText);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    QList<int> m_clipIds;
    QList<ClipData> m_snapshots; // 削除されたクリップの復元用スナップショット
};

class PasteClipCommand : public QUndoCommand {
  public:
    PasteClipCommand(TimelineService *service, int newClipId, const ClipData &clipData);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_newClipId;
    ClipData m_clipData;
};

class CutClipCommand : public QUndoCommand {
  public:
    CutClipCommand(TimelineService *service, int clipId, const QString &clipName);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId;
    ClipData m_snapshot;
};

class SetKeyframeCommand : public QUndoCommand {
  public:
    SetKeyframeCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, int frame, QVariant newValue, QVariantMap options, QVariant oldValue, QVariantMap oldOptions, bool wasExisting);
    void undo() override;
    void redo() override;
    int id() const override;
    bool mergeWith(const QUndoCommand *other) override;

  private:
    TimelineService *m_service;
    int m_clipId, m_effectIndex, m_frame;
    QString m_paramName;
    QVariant m_newValue, m_oldValue;
    QVariantMap m_newOptions, m_oldOptions;
    bool m_wasExisting;
};

class RemoveKeyframeCommand : public QUndoCommand {
  public:
    RemoveKeyframeCommand(TimelineService *service, int clipId, int effectIndex, const QString &paramName, int frame, QVariant savedValue, QVariantMap savedOptions);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_clipId, m_effectIndex, m_frame;
    QString m_paramName;
    QVariant m_savedValue;
    QVariantMap m_savedOptions;
};

class AddSceneCommand : public QUndoCommand {
  public:
    AddSceneCommand(TimelineService *service, int sceneId, const QString &name);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_sceneId;
    QString m_name;
};

class RemoveSceneCommand : public QUndoCommand {
  public:
    RemoveSceneCommand(TimelineService *service, int sceneId, const QString &name);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_sceneId;
    SceneData m_snapshot;
};

class UpdateSceneSettingsCommand : public QUndoCommand {
  public:
    UpdateSceneSettingsCommand(TimelineService *service, int sceneId, SceneData oldData, const SceneData &newData);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_sceneId;
    SceneData m_oldData, m_newData;
};

class UpdateLayerStateCommand : public QUndoCommand {
  public:
    enum StateType { Lock, Hidden };
    UpdateLayerStateCommand(TimelineService *service, int sceneId, int layer, bool value, StateType type);
    void undo() override;
    void redo() override;

  private:
    TimelineService *m_service;
    int m_sceneId, m_layer;
    bool m_value;
    StateType m_type;
};

} // namespace AviQtl::UI