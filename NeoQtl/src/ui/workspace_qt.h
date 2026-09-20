#pragma once

#include <QtCore/QObject>
#include <QtCore/QVariantList>
#include <QtCore/QUrl>
#include <QtQml/QQmlApplicationEngine>

class WorkspaceBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList tabs READ tabs NOTIFY tabsChanged)
    Q_PROPERTY(int currentIndex READ currentIndex WRITE setCurrentIndex NOTIFY currentIndexChanged)
    Q_PROPERTY(QObject *currentTimeline READ currentTimeline NOTIFY currentTimelineChanged)
    Q_PROPERTY(int currentSceneId READ currentSceneId NOTIFY currentSceneIdChanged)

public:
    explicit WorkspaceBridge(QObject *parent = nullptr);

    QVariantList tabs() const;
    int currentIndex() const;
    void setCurrentIndex(int index);
    QObject *currentTimeline() const;
    int currentSceneId() const;

    Q_INVOKABLE bool newProject();
    Q_INVOKABLE bool loadProject(const QUrl &url);
    Q_INVOKABLE bool closeProject(int index);

Q_SIGNALS:
    void tabsChanged();
    void currentIndexChanged();
    void currentTimelineChanged();
    void currentSceneIdChanged();
};

void register_workspace_bridge(QQmlApplicationEngine &engine);
