#pragma once

#include <QtCore/QObject>
#include <QtCore/QVariantList>
#include <QtCore/QVariantMap>
#include <QtQml/QQmlApplicationEngine>

class TimelineBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList clips READ clips NOTIFY clipsChanged)
    Q_PROPERTY(QVariantList sceneTabs READ sceneTabs NOTIFY scenesChanged)
    Q_PROPERTY(int currentFrame READ currentFrame NOTIFY transportChanged)
    Q_PROPERTY(int totalFrames READ totalFrames NOTIFY transportChanged)
    Q_PROPERTY(int layerCount READ layerCount NOTIFY clipsChanged)
    Q_PROPERTY(qreal timelineScale READ timelineScale NOTIFY viewChanged)
    Q_PROPERTY(int projectFps READ projectFps NOTIFY transportChanged)
    Q_PROPERTY(bool enableSnap READ enableSnap NOTIFY sceneSettingsChanged)
    Q_PROPERTY(int magneticSnapRange READ magneticSnapRange NOTIFY sceneSettingsChanged)
    Q_PROPERTY(QVariantMap gridSettings READ gridSettings NOTIFY sceneSettingsChanged)

public:
    explicit TimelineBridge(QObject *parent = nullptr);

    QVariantList clips() const;
    QVariantList sceneTabs() const;
    int currentFrame() const;
    int totalFrames() const;
    int layerCount() const;
    qreal timelineScale() const;
    int projectFps() const;
    bool enableSnap() const;
    int magneticSnapRange() const;
    QVariantMap gridSettings() const;

    Q_INVOKABLE void setViewport(qreal contentX, qreal contentY, qreal width, qreal height);
    Q_INVOKABLE void setZoom(qreal value);
    Q_INVOKABLE void seek(int frame);

    Q_INVOKABLE void selectClip(int id, bool additive);
    Q_INVOKABLE void clearSelection();
    Q_INVOKABLE void selectInRect(qreal x0, qreal y0, qreal x1, qreal y1);

    Q_INVOKABLE void applyClipBatchMove(int clipId, int deltaLayer, int deltaStartFrame);
    Q_INVOKABLE void applyClipResize(int clipId, int deltaStartFrame, int deltaDuration);
    Q_INVOKABLE void splitClip(int id, int frame);
    Q_INVOKABLE void deleteSelected();
    Q_INVOKABLE void moveKeyframe(int id, int fromFrame, int toFrame);

    Q_INVOKABLE bool switchScene(int sceneId);
    Q_INVOKABLE int addScene(const QString &name);
    Q_INVOKABLE bool removeScene(int sceneId);

Q_SIGNALS:
    void clipsChanged();
    void scenesChanged();
    void transportChanged();
    void viewChanged();
    void sceneSettingsChanged();
};

void register_timeline_bridge(QQmlApplicationEngine &engine);
TimelineBridge* timeline_bridge_instance();