#pragma once

#include <QtCore/QObject>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>
#include <QtQml/QQmlApplicationEngine>

class TransportBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool isPlaying READ isPlaying NOTIFY transportChanged)
    Q_PROPERTY(bool isScrubbing READ isScrubbing NOTIFY transportChanged)
    Q_PROPERTY(int currentFrame READ currentFrame NOTIFY transportChanged)
    Q_PROPERTY(int totalFrames READ totalFrames NOTIFY transportChanged)
    Q_PROPERTY(qreal playbackSpeed READ playbackSpeed NOTIFY transportChanged)

public:
    explicit TransportBridge(QObject *parent = nullptr);

    bool isPlaying() const;
    bool isScrubbing() const;
    int currentFrame() const;
    int totalFrames() const;
    qreal playbackSpeed() const;

    Q_INVOKABLE void togglePlay();
    Q_INVOKABLE void pause();
    Q_INVOKABLE void setCurrentFrame_seek(int frame);
    Q_INVOKABLE void scrubTo(int frame);
    Q_INVOKABLE void beginScrub();
    Q_INVOKABLE void endScrub();

Q_SIGNALS:
    void transportChanged();
};

class TimelineBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList clips READ clips NOTIFY clipsChanged)
    Q_PROPERTY(QVariantList sceneTabs READ sceneTabs NOTIFY scenesChanged)
    Q_PROPERTY(QVariantList scenes READ scenes NOTIFY scenesChanged)
    Q_PROPERTY(int currentFrame READ currentFrame NOTIFY transportChanged)
    Q_PROPERTY(int totalFrames READ totalFrames NOTIFY transportChanged)
    Q_PROPERTY(int layerCount READ layerCount NOTIFY clipsChanged)
    Q_PROPERTY(qreal timelineScale READ timelineScale WRITE setTimelineScale NOTIFY viewChanged)
    Q_PROPERTY(int projectFps READ projectFps NOTIFY transportChanged)
    Q_PROPERTY(bool enableSnap READ enableSnap NOTIFY sceneSettingsChanged)
    Q_PROPERTY(int magneticSnapRange READ magneticSnapRange NOTIFY sceneSettingsChanged)
    Q_PROPERTY(QVariantMap gridSettings READ gridSettings NOTIFY sceneSettingsChanged)

    Q_PROPERTY(QObject *transport READ transport CONSTANT)
    Q_PROPERTY(QVariantMap project READ project NOTIFY sceneSettingsChanged)
    Q_PROPERTY(QVariantMap selection READ selection NOTIFY selectionChanged)
    Q_PROPERTY(QVariantList previewSelectionIds READ previewSelectionIds NOTIFY selectionChanged)
    Q_PROPERTY(int cursorFrame READ cursorFrame WRITE setCursorFrame NOTIFY transportChanged)
    Q_PROPERTY(int selectedLayer READ selectedLayer WRITE setSelectedLayer NOTIFY selectionChanged)
    Q_PROPERTY(int timelineDuration READ timelineDuration NOTIFY transportChanged)
    Q_PROPERTY(int currentSceneId READ currentSceneId NOTIFY currentSceneIdChanged)
    Q_PROPERTY(int clipStartFrame READ clipStartFrame NOTIFY selectionChanged)
    Q_PROPERTY(int clipDurationFrames READ clipDurationFrames NOTIFY selectionChanged)
    Q_PROPERTY(QString currentProjectUrl READ currentProjectUrl NOTIFY projectChanged)
    Q_PROPERTY(bool hasUnsavedChanges READ hasUnsavedChanges NOTIFY projectChanged)
    Q_PROPERTY(bool isExporting READ isExporting NOTIFY exportChanged)

public:
    explicit TimelineBridge(QObject *parent = nullptr);

    QVariantList clips() const;
    QVariantList sceneTabs() const;
    QVariantList scenes() const;
    int currentFrame() const;
    int totalFrames() const;
    int layerCount() const;
    qreal timelineScale() const;
    void setTimelineScale(qreal value);
    int projectFps() const;
    bool enableSnap() const;
    int magneticSnapRange() const;
    QVariantMap gridSettings() const;

    QObject *transport() const;
    QVariantMap project() const;
    QVariantMap selection() const;
    QVariantList previewSelectionIds() const;
    int cursorFrame() const;
    void setCursorFrame(int frame);
    int selectedLayer() const;
    void setSelectedLayer(int layer);
    int timelineDuration() const;
    int currentSceneId() const;
    int clipStartFrame() const;
    int clipDurationFrames() const;
    QString currentProjectUrl() const;
    bool hasUnsavedChanges() const;
    bool isExporting() const;

    Q_INVOKABLE void setViewport(qreal contentX, qreal contentY, qreal width, qreal height);
    Q_INVOKABLE void updateViewport(qreal contentX, qreal contentY);
    Q_INVOKABLE void setZoom(qreal value);
    Q_INVOKABLE void seek(int frame);
    Q_INVOKABLE void togglePlay();
    Q_INVOKABLE void syncPlaybackSpeed();
    Q_INVOKABLE void setCompositeView(bool enabled);

    Q_INVOKABLE void selectClip(int id, bool additive);
    Q_INVOKABLE void clearSelection();
    Q_INVOKABLE void selectInRect(qreal x0, qreal y0, qreal x1, qreal y1);
    Q_INVOKABLE void applySelectionIds(const QVariantList &ids);
    Q_INVOKABLE void handleClipClick(int id, int modifiers);
    Q_INVOKABLE void updateSelectionPreview(int frame0, int frame1, int layer0, int layer1, bool additive);
    Q_INVOKABLE void clearSelectionPreview();
    Q_INVOKABLE void finalizeSelectionPreview();

    Q_INVOKABLE void applyClipBatchMove(const QVariant &moves);
    Q_INVOKABLE void applyClipResize(int clipId, int deltaStartFrame, int deltaDuration);
    Q_INVOKABLE void updateClip(int clipId, int layer, int startFrame, int durationFrames);
    Q_INVOKABLE void deleteClip(int clipId);
    Q_INVOKABLE void deleteSelected();
    Q_INVOKABLE void deleteSelectedClips();
    Q_INVOKABLE void copyClip(int clipId);
    Q_INVOKABLE void copySelectedClips();
    Q_INVOKABLE void cutClip(int clipId);
    Q_INVOKABLE void cutSelectedClips();
    Q_INVOKABLE QVariantList pasteClip(int frame, int layer);
    Q_INVOKABLE void splitClip(int id, int frame);
    Q_INVOKABLE void splitSelectedClips(int frame);
    Q_INVOKABLE void moveSelectedClips(int deltaFrame, int deltaLayer);
    Q_INVOKABLE void resizeSelectedClips(int deltaStartFrame, int deltaDuration);
    Q_INVOKABLE QVariantMap resolveDragDelta(int clipId, int deltaFrame, int deltaLayer,
                                             const QVariantList &activeIds, int selectionMinFrame,
                                             int selectionMinLayer, int selectionMaxLayer,
                                             int layerCount);
    Q_INVOKABLE bool clipByUpperObject(int clipId) const;
    Q_INVOKABLE void setClipByUpperObject(int clipId, bool enabled);
    Q_INVOKABLE QString getClipTypeColor(int type) const;
    Q_INVOKABLE bool isAudioClip(int clipId) const;

    Q_INVOKABLE void setKeyframe(int clipId, int effectIndex, const QString &key, int frame,
                                 qreal value, const QVariantMap &options);
    Q_INVOKABLE void removeKeyframe(int clipId, int effectIndex, const QString &key, int frame);
    Q_INVOKABLE void moveKeyframe(int clipId, int effectIndex, const QString &key, int fromFrame,
                                  int toFrame);
    Q_INVOKABLE QVariantList getWaveformPeaks(int clipId, int width, int durationFrames) const;

    Q_INVOKABLE QVariantList getAvailableEffects() const;
    Q_INVOKABLE QVariantList getClipEffectStack(int clipId) const;
    Q_INVOKABLE QVariantList getClipEffectsModel(int clipId) const;
    Q_INVOKABLE int getClipEffectIndex(int clipId, const QString &effectId) const;
    Q_INVOKABLE QVariantList getEffectParameters(int clipId, int effectIndex) const;
    Q_INVOKABLE void addEffect(int clipId, const QString &effectId);
    Q_INVOKABLE void removeEffect(int clipId, int effectIndex);
    Q_INVOKABLE void removeMultipleEffects(int clipId, const QVariantList &effectIndices);
    Q_INVOKABLE void reorderEffects(int clipId, int fromIndex, int toIndex);
    Q_INVOKABLE void reorderMultipleEffects(int clipId, const QVariantList &fromIndices, int toIndex);
    Q_INVOKABLE void setEffectEnabled(int clipId, int effectIndex, bool enabled);
    Q_INVOKABLE void setEffectParameter(int clipId, int effectIndex, const QString &key, qreal value);
    Q_INVOKABLE void updateClipEffectParam(int clipId, int effectIndex, const QString &key, qreal value);

    Q_INVOKABLE bool addAudioPlugin(int clipId, const QString &pluginId);
    Q_INVOKABLE void removeAudioPlugin(int clipId, int pluginIndex);
    Q_INVOKABLE void setAudioPluginEnabled(int clipId, int pluginIndex, bool enabled);
    Q_INVOKABLE void reorderAudioPlugins(int clipId, int fromIndex, int toIndex);
    Q_INVOKABLE QVariantList getPluginCategories() const;
    Q_INVOKABLE QVariantList getPluginsByCategory(const QString &category) const;
    Q_INVOKABLE void updateAudioSampleRate();

    Q_INVOKABLE bool isLayerHidden(int layer) const;
    Q_INVOKABLE bool isLayerLocked(int layer) const;
    Q_INVOKABLE void setLayerState(int layer, bool value, int field);
    Q_INVOKABLE void insertLayers(int layerIndex, int count, bool above);
    Q_INVOKABLE void shiftLayers(int fromLayer, int toLayer, int delta);

    Q_INVOKABLE bool switchScene(int sceneId);
    Q_INVOKABLE int addScene(const QString &name);
    Q_INVOKABLE int createScene(const QString &name);
    Q_INVOKABLE bool removeScene(int sceneId);
    Q_INVOKABLE QVariantList getSceneClips(int sceneId) const;
    Q_INVOKABLE int getSceneDuration(int sceneId) const;
    Q_INVOKABLE QVariantMap getSceneInfo(int sceneId) const;
    Q_INVOKABLE bool updateSceneSettings(int sceneId, const QString &name, int width, int height,
                                         qreal fps, int totalFrames, int gridMode, qreal bpm);

    Q_INVOKABLE QVariantList getAvailableObjects() const;
    Q_INVOKABLE int createObject(const QString &stableId, int frame, int layer);
    Q_INVOKABLE bool saveProject(const QString &path);
    Q_INVOKABLE bool undo();
    Q_INVOKABLE bool redo();
    Q_INVOKABLE bool exportVideoAsync(const QVariantMap &options);
    Q_INVOKABLE bool cancelExport();

Q_SIGNALS:
    void clipsChanged();
    void scenesChanged();
    void currentSceneIdChanged();
    void transportChanged();
    void viewChanged();
    void sceneSettingsChanged();
    void selectionChanged();
    void projectChanged();
    void exportChanged();
    void effectsChanged();
};

void register_timeline_bridge(QQmlApplicationEngine &engine);
TimelineBridge *timeline_bridge_instance();
TransportBridge *transport_bridge_instance();
