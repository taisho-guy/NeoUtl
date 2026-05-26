#pragma once
#include "../../engine/audio_mixer.hpp"
#include "effect_model.hpp"
#include "project_service.hpp"
#include "selection_service.hpp"
#include "timeline_export_manager.hpp"
#include "timeline_media_manager.hpp"
#include "timeline_service.hpp"
#include "timeline_types.hpp"
#include "transport_service.hpp"
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QQuickItem>
#include <QVariant>
#include <memory>
#include <vector>

namespace AviQtl::Core {
class VideoFrameStore;
}

namespace AviQtl::UI { // 元のnamespaceに戻す
class TimelineController : public QObject {
    Q_OBJECT

    // === サービス (サブコントローラ) ===
    Q_PROPERTY(AviQtl::UI::ProjectService *project READ project CONSTANT)
    Q_PROPERTY(AviQtl::UI::TransportService *transport READ transport CONSTANT)
    Q_PROPERTY(AviQtl::UI::SelectionService *selection READ selection CONSTANT)
    Q_PROPERTY(int cursorFrame READ cursorFrame WRITE setCursorFrame NOTIFY cursorFrameChanged)

    // === レガシー / ファサードプロパティ ===
    Q_PROPERTY(double timelineScale READ timelineScale WRITE setTimelineScale NOTIFY timelineScaleChanged)
    Q_PROPERTY(int clipStartFrame READ clipStartFrame WRITE setClipStartFrame NOTIFY clipStartFrameChanged)
    Q_PROPERTY(int clipDurationFrames READ clipDurationFrames WRITE setClipDurationFrames NOTIFY clipDurationFramesChanged)
    Q_PROPERTY(int layer READ layer WRITE setLayer NOTIFY layerChanged)
    Q_PROPERTY(bool isClipActive READ isClipActive NOTIFY isClipActiveChanged)
    Q_PROPERTY(QString activeObjectType READ activeObjectType NOTIFY activeObjectTypeChanged)
    Q_PROPERTY(QVariantList clips READ clips NOTIFY clipsChanged)
    Q_PROPERTY(int timelineDuration READ timelineDuration NOTIFY timelineDurationChanged)
    Q_PROPERTY(int selectedLayer READ selectedLayer WRITE setSelectedLayer NOTIFY selectedLayerChanged)
    Q_PROPERTY(QVariantList scenes READ scenes NOTIFY scenesChanged)
    Q_PROPERTY(int currentSceneId READ currentSceneId NOTIFY currentSceneIdChanged)
    Q_PROPERTY(QString currentProjectUrl READ currentProjectUrl NOTIFY currentProjectUrlChanged)
    Q_PROPERTY(bool hasUnsavedChanges READ hasUnsavedChanges NOTIFY hasUnsavedChangesChanged)
    Q_PROPERTY(QVariantList previewSelectionIds READ previewSelectionIds NOTIFY previewSelectionIdsChanged)

  public:
    explicit TimelineController(QObject *parent = nullptr);

    void setVideoFrameStore(AviQtl::Core::VideoFrameStore *store);

    Q_INVOKABLE void setCompositeView(QQuickItem *view) { m_compositeView = view; }
    QQuickItem *compositeView() const { return m_compositeView; }

    // サービスアクセサ
    ProjectService *project() const { return m_project; }
    TransportService *transport() const { return m_transport; }
    SelectionService *selection() const { return m_selection; }
    TimelineService *timeline() const { return m_timeline; }
    TimelineMediaManager *mediaManager() const { return m_mediaManager; }

    double timelineScale() const;
    void setTimelineScale(double scale);

    // 汎用プロパティ操作
    Q_INVOKABLE void setClipProperty(const QString &name, const QVariant &value);
    Q_INVOKABLE QVariant getClipProperty(const QString &name) const;

    int clipStartFrame() const;
    void setClipStartFrame(int frame);

    int clipDurationFrames() const;
    void setClipDurationFrames(int frames);

    int layer() const;
    void setLayer(int layer);

    int cursorFrame() const { return m_cursorFrame; }
    void setCursorFrame(int frame);

    int selectedLayer() const { return m_selectedLayer; }
    void setSelectedLayer(int layer);

    bool isClipActive() const;

    Q_INVOKABLE void createObject(const QString &type, int startFrame, int layer);
    QString activeObjectType() const;

    Q_INVOKABLE static void log(const QString &msg);
    QVariantList clips() const;

    // クリップの配置・長さを更新 (ID指定)
    Q_INVOKABLE void updateClip(int id, int layer, int startFrame, int duration);
    Q_INVOKABLE bool clipByUpperObject(int clipId) const;
    Q_INVOKABLE void setClipByUpperObject(int clipId, bool enabled);
    Q_INVOKABLE void insertLayers(int targetLayer, int count, bool above);
    Q_INVOKABLE void shiftLayers(int startLayer, int endLayer, int delta);
    Q_INVOKABLE void moveSelectedClips(int deltaLayer, int deltaFrame);
    Q_INVOKABLE void applyClipBatchMove(const QVariantList &moves);
    Q_INVOKABLE void resizeSelectedClips(int deltaStartFrame, int deltaDuration);

    Q_INVOKABLE QVariantMap evaluateClipParams(int clipId, int relFrame) const;

    // エフェクト操作
    Q_INVOKABLE QList<QObject *> getClipEffectsModel(int clipId) const;
    Q_INVOKABLE int getClipEffectIndex(int clipId, QObject *effectModel) const;
    Q_INVOKABLE void updateClipEffectParam(int clipId, int effectIndex, const QString &paramName, const QVariant &value);

    // エフェクト・オブジェクトの利用可能リスト取得
    Q_INVOKABLE static QVariantList getAvailableEffects();
    Q_INVOKABLE static QVariantList getAvailableObjects();
    Q_INVOKABLE static QString getClipTypeColor(const QString &type);
    Q_INVOKABLE void addEffect(int clipId, const QString &effectId);
    Q_INVOKABLE void removeEffect(int clipId, int effectIndex);
    Q_INVOKABLE void removeMultipleEffects(int clipId, const QList<int> &indices);
    Q_INVOKABLE void copyEffect(int clipId, int effectIndex);
    Q_INVOKABLE void pasteEffect(int clipId, int targetIndex);
    Q_INVOKABLE void cutEffect(int clipId, int effectIndex);
    Q_INVOKABLE void setEffectEnabled(int clipId, int effectIndex, bool enabled);
    Q_INVOKABLE void reorderEffects(int clipId, int oldIndex, int newIndex);
    Q_INVOKABLE void reorderMultipleEffects(int clipId, const QVariantList &indicesList, int targetIndex);

    // レイヤー操作
    Q_INVOKABLE bool isLayerLocked(int layer) const { return m_timeline->isLayerLocked(layer); }
    Q_INVOKABLE bool isLayerHidden(int layer) const { return m_timeline->isLayerHidden(layer); }
    Q_INVOKABLE void setLayerState(int layer, bool value, int type) { m_timeline->setLayerState(layer, value, type); }

    // オーディオプラグイン操作
    Q_INVOKABLE static QVariantList getAvailableAudioPlugins();
    Q_INVOKABLE void addAudioPlugin(int clipId, const QString &pluginId);
    Q_INVOKABLE void removeAudioPlugin(int clipId, int index);
    Q_INVOKABLE void setAudioPluginEnabled(int clipId, int index, bool enabled);
    Q_INVOKABLE void reorderAudioPlugins(int clipId, int oldIndex, int newIndex);
    Q_INVOKABLE static QVariantList getPluginCategories();
    Q_INVOKABLE static QVariantList getPluginsByCategory(const QString &category);
    Q_INVOKABLE bool isAudioClip(int clipId) const;
    Q_INVOKABLE QVariantList getWaveformPeaks(int clipId, int pixelWidth, int displayDurationFrames) const;

    // パラメータ操作用
    Q_INVOKABLE QVariantList getClipEffectStack(int clipId) const;
    Q_INVOKABLE QVariantList getEffectParameters(int clipId, int effectIndex) const;
    Q_INVOKABLE void setEffectParameter(int clipId, int effectIndex, int paramIndex, float value);
    Q_INVOKABLE void setKeyframe(int clipId, int effectIndex, const QString &paramName, int frame, const QVariant &value, const QVariantMap &options);
    Q_INVOKABLE void removeKeyframe(int clipId, int effectIndex, const QString &paramName, int frame);
    Q_INVOKABLE void moveKeyframe(int clipId, int effectIndex, const QString &paramName, int oldFrame, int newFrame);

    // シーン操作
    QVariantList scenes() const;
    int currentSceneId() const;
    QString currentProjectUrl() const { return m_currentProjectUrl; }
    bool hasUnsavedChanges() const;
    Q_INVOKABLE void createScene(const QString &name);
    Q_INVOKABLE void removeScene(int sceneId);
    Q_INVOKABLE void switchScene(int sceneId);
    Q_INVOKABLE void updateSceneSettings(int sceneId, const QString &name, int width, int height, double fps, int totalFrames, const QString &gridMode, double gridBpm, double gridOffset, int gridInterval, int gridSubdivision, bool enableSnap,
                                         int magneticSnapRange);
    Q_INVOKABLE QVariantList getSceneClips(int sceneId) const;
    Q_INVOKABLE QVariantMap getSceneInfo(int sceneId) const;
    Q_INVOKABLE int getSceneDuration(int sceneId) const;
    Q_INVOKABLE void requestVideoFrame(int clipId, int relFrame);
    Q_INVOKABLE void requestImageLoad(int clipId, const QString &path);

    Q_INVOKABLE static void updateViewport(double x, double y);
    Q_INVOKABLE QPoint resolveDragPosition(int clipId, int targetLayer, int proposedStartFrame, const QVariantList &batchIds = QVariantList());

    // プロジェクトI/O
    Q_INVOKABLE bool saveProject(const QString &fileUrl);
    Q_INVOKABLE bool loadProject(const QString &fileUrl);
    // 新非同期インターフェース
    Q_INVOKABLE void exportVideoAsync(const QVariantMap &config);
    Q_INVOKABLE void cancelExport();
    Q_PROPERTY(bool isExporting READ isExporting NOTIFY exportFinished)
    bool isExporting() const;

    Q_INVOKABLE void handleClipClick(int clipId, int modifiers);
    Q_INVOKABLE void updateSelectionPreview(int frameA, int frameB, int layerA, int layerB, bool additive);
    Q_INVOKABLE void finalizeSelectionPreview();
    Q_INVOKABLE void clearSelectionPreview();
    Q_INVOKABLE QVariantList previewSelectionIds() const;

    Q_INVOKABLE void selectClip(int id);
    Q_INVOKABLE void toggleSelection(int id, const QVariantMap &data);
    Q_INVOKABLE void applySelectionIds(const QVariantList &ids);

    Q_INVOKABLE QPoint resolveDragDelta(int clipId, int deltaFrame, int deltaLayer, const QVariantList &batchIds, int minFrame, int minLayer, int maxLayer, int totalLayers);

    Q_INVOKABLE void togglePlay();
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();

    Q_INVOKABLE void requestDelete(int targetClipId);
    Q_INVOKABLE void deleteClip(int clipId);
    Q_INVOKABLE void splitClip(int clipId, int frame);
    Q_INVOKABLE void splitSelectedClips(int frame);
    Q_INVOKABLE void copyClip(int clipId);
    Q_INVOKABLE void cutClip(int clipId);
    Q_INVOKABLE void pasteClip(int frame, int layer);
    Q_INVOKABLE void deleteSelectedClips();
    Q_INVOKABLE void copySelectedClips();
    Q_INVOKABLE void cutSelectedClips();

    void updateActiveClipsList();

    Q_INVOKABLE void syncPlaybackSpeed() { m_mediaManager->syncPlaybackSpeed(); }

    Q_INVOKABLE void updateAudioSampleRate() { m_mediaManager->updateAudioSampleRate(); }

    // 動的に計算されたタイムラインの長さ（最後のクリップの末尾フレーム）
    int timelineDuration() const { return m_cachedTimelineDuration; }
    void invalidateTimelineDuration();

  signals:
    void videoFrameRequested(int clipId, int relFrame);
    void imageLoadRequested(int clipId, const QString &path);
    void timelineScaleChanged();
    void clipStartFrameChanged();
    void clipDurationFramesChanged();
    void layerChanged();
    void cursorFrameChanged();
    void isClipActiveChanged();
    void activeObjectTypeChanged(); // 選択中クリップの種別 (text, rectなど)
    void clipsChanged();
    void effectParamChanged(int clipId, int effectIndex, const QString &paramName, const QVariant &value); // 追加
    void scenesChanged();
    void currentSceneIdChanged();
    void currentProjectUrlChanged();
    void hasUnsavedChangesChanged();
    void clipEffectsChanged(int clipId);
    void previewSelectionIdsChanged();
    void selectedLayerChanged();
    void errorOccurred(const QString &message);
    void exportStarted(int totalFrames);
    void exportProgressChanged(int progress, int currentFrame, int totalFrames);
    void exportFinished(bool success, const QString &message);
    void timelineDurationChanged();

  private:
    // Initialization Helpers
    void initializeServices();
    void setupConnections();
    void syncTimelineToDocumentModel();

    // Internal Slots
    void onPlayingChanged();
    void onCurrentFrameChanged();

    int clampedDuration(int clipId, int newStart, int requestedDuration) const;

    void updateClipActiveState();
    double m_timelineScale = 1.0; // タイムラインの表示倍率 (1.0 = 1フレームあたり1ピクセル)

    bool m_isClipActive = false;
    int m_selectedLayer = 0;
    QString m_currentProjectUrl;

    int m_cursorFrame = 0;
    // 各機能を担当するサービス群
    ProjectService *m_project{};
    TransportService *m_transport{};
    SelectionService *m_selection{};
    TimelineService *m_timeline{};

    TimelineMediaManager *m_mediaManager{};
    TimelineExportManager *m_exportManager{};

    QVariantList m_previewSelectionIds;

  private:
    QPointer<QQuickItem> m_compositeView; // CompositeViewへの参照

    // キャッシュ: タイムラインの長さ（最大クリップ末尾フレーム）
    // clipsChanged / sceneChanged 時に再計算される
    mutable int m_cachedTimelineDuration = 300;
};
} // namespace AviQtl::UI
