#pragma once
#include "timeline_types.hpp"
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QSet>
#include <QUndoStack>
#include <memory>

namespace AviQtl::UI {
class SelectionService;

class TimelineService : public QObject {
    Q_OBJECT
  public:
    explicit TimelineService(SelectionService *selection, QObject *parent = nullptr);
    virtual ~TimelineService();

    // データアクセス
    const QList<ClipData> &clips() const;
    QList<ClipData> &clipsMutable();                 // シリアライザ用
    const QList<ClipData> &clips(int sceneId) const; // 特定シーンのクリップ取得

    // ネストを解決した「フレーム時点のアクティブクリップ」を返す
    QList<ClipData *> resolvedActiveClipsAt(int frame) const;

    // 指定された条件で配置可能な最短の開始フレームを計算する（衝突回避）
    int findVacantFrame(int layer, int startFrame, int duration, int excludeClipId) const;

    const QList<SceneData> &getAllScenes() const { return m_scenes; }
    void setScenes(const QList<SceneData> &scenes);
    QUndoStack *undoStack() const { return m_undoStack; }

    // 操作 (公開API)
    void undo();
    void redo();
    void createClip(const QString &type, int startFrame, int layer);
    void deleteClip(int clipId);
    void updateClip(int id, int layer, int startFrame, int duration);
    void moveSelectedClips(int deltaLayer, int deltaFrame);
    void applyClipBatchMove(const QVariantList &moves);
    void resizeSelectedClips(int deltaStartFrame, int deltaDuration);
    void splitClip(int clipId, int frame);
    void splitSelectedClips(int frame);
    Q_INVOKABLE int computeMagneticSnapPosition(int clipId, int targetLayer, int proposedStartFrame); // Note: This is for a different snap feature
    Q_INVOKABLE QPoint resolveDragPosition(int clipId, int targetLayer, int proposedStartFrame, const QVariantList &batchIds = QVariantList());
    void selectClip(int id);
    void selectClipsInRange(int frameA, int frameB, int layerA, int layerB, bool additive = false);
    void toggleSelection(int id, const QVariantMap &data);
    void applySelectionIds(const QVariantList &ids);

    // シーン管理
    QVariantList scenes() const;
    int currentSceneId() const { return m_currentSceneId; }
    void createScene(const QString &name);
    void removeScene(int sceneId);
    void switchScene(int sceneId);
    void updateSceneSettings(int sceneId, const QString &name, int width, int height, double fps, int totalFrames, const QString &gridMode, double gridBpm, double gridOffset, int gridInterval, int gridSubdivision, bool enableSnap, int magneticSnapRange);

    // レイヤー状態操作
    Q_INVOKABLE bool isLayerLocked(int layer) const;
    Q_INVOKABLE bool isLayerHidden(int layer) const;
    void setLayerState(int layer, bool value, int type); // 0: Lock, 1: Hidden

    // エフェクト
    void addEffect(int clipId, const QString &effectId);
    void removeEffect(int clipId, int effectIndex);
    void removeMultipleEffects(int clipId, const QList<int> &indices);
    void setEffectEnabled(int clipId, int effectIndex, bool enabled);
    void setAudioPluginEnabled(int clipId, int index, bool enabled);
    void reorderEffects(int clipId, int oldIndex, int newIndex);
    void reorderMultipleEffects(int clipId, const QVariantList &indicesList, int targetIndex);
    void reorderAudioPlugins(int clipId, int oldIndex, int newIndex);
    void copyEffect(int clipId, int effectIndex);
    void pasteEffect(int clipId, int targetIndex);
    void updateEffectParam(int clipId, int effectIndex, const QString &paramName, const QVariant &value);
    void setKeyframe(int clipId, int effectIndex, const QString &paramName, int frame, const QVariant &value, const QVariantMap &options);
    void removeKeyframe(int clipId, int effectIndex, const QString &paramName, int frame);

    // クリップボード
    void copyClip(int clipId);
    void cutClip(int clipId);
    void pasteClip(int frame, int layer);
    void copySelectedClips();
    void cutSelectedClips();
    void deleteSelectedClips();
    void deleteClipsByIds(const QVariantList &ids);

    // 内部用 (コマンドから呼び出される)
    void deleteClipInternal(int clipId, bool emitSignal = true);
    void createClipInternal(int clipId, const QString &type, int startFrame, int layer, bool emitSignal = true);
    void updateClipInternal(int id, int layer, int startFrame, int duration, bool emitSignal = true);
    void addEffectInternal(int clipId, const QString &effectId);
    void addClipsDirectInternal(const QList<ClipData> &clips);
    void addClipDirectInternal(const ClipData &clip, bool emitSignal = true);
    void restoreEffectInternal(int clipId, const QVariantMap &data);
    void removeEffectInternal(int clipId, int effectIndex);
    void removeMultipleEffectsInternal(int clipId, const QList<int> &sortedDescIndices, QList<QVariantMap> *outData);
    void restoreMultipleEffectsInternal(int clipId, const QList<QVariantMap> &ascData);
    void setEffectEnabledInternal(int clipId, int effectIndex, bool enabled);
    void pasteEffectInternal(int clipId, int targetIndex, EffectModel *effect);
    void setAudioPluginEnabledInternal(int clipId, int index, bool enabled);
    void reorderEffectsInternal(int clipId, int oldIndex, int newIndex);
    void applyPermutationInternal(int clipId, const QList<int> &perm);
    void reorderAudioPluginsInternal(int clipId, int oldIndex, int newIndex);
    void updateEffectParamInternal(int clipId, int effectIndex, const QString &paramName, const QVariant &value);
    void setClipboard(const ClipData &clip);
    void setClipboard(const QList<ClipData> &clips);
    void createSceneInternal(int sceneId, const QString &name);
    void removeSceneInternal(int sceneId);
    void restoreSceneInternal(const SceneData &scene);
    void applySceneSettingsInternal(int sceneId, const SceneData &data);
    void setKeyframeInternal(int clipId, int effectIndex, const QString &paramName, int frame, const QVariant &value, const QVariantMap &options);
    void removeKeyframeInternal(int clipId, int effectIndex, const QString &paramName, int frame);
    void setLayerStateInternal(int sceneId, int layer, bool value, int type);
    ClipData *findClipById(int clipId);
    const ClipData *findClipById(int clipId) const;

    // ヘルパー
    ClipData deepCopyClip(const ClipData &source);

    // 状態管理
    int nextClipId() const { return m_nextClipId; }
    void setNextClipId(int id) { m_nextClipId = id; }
    int nextSceneId() const { return m_nextSceneId; }
    void setNextSceneId(int id) { m_nextSceneId = id; }

  signals:
    void clipsChanged();
    void scenesChanged();
    void currentSceneIdChanged();
    void clipEffectsChanged(int clipId);
    void layerStateChanged(int layer);
    void effectParamChanged(int clipId, int effectIndex, const QString &paramName, const QVariant &value);
    void clipCreated(int id, int layer, int startFrame, int duration, const QString &type);

  private:
    QList<SceneData> m_scenes;
    int m_currentSceneId = 0;

    SceneData *currentScene();
    const SceneData *currentScene() const;

    int m_nextClipId = 1;
    int m_nextSceneId = 1;
    QUndoStack *m_undoStack;
    QList<ClipData> m_clipboard;
    std::unique_ptr<EffectModel> m_effectClipboard;
    SelectionService *m_selection;
    QSet<int> m_batchExcludes;
};
} // namespace AviQtl::UI
