#include "workspace_qt.h"
#include "bridge_qt.h"
#include "NeoQtl/src/ui/workspace_bridge.cxx.h"
#include "NeoQtl/src/ui/timeline/bridge/timeline_bridge.cxx.h"

#include <QtQml/qqml.h>
#include <QtQml/QQmlContext>

WorkspaceBridge::WorkspaceBridge(QObject *parent) : QObject(parent) {}

QVariantList WorkspaceBridge::tabs() const {
    QVariantList out;
    for (const auto &tab : neoqtl::workspace_tabs()) {
        QVariantMap m;
        m["name"] = QString::fromUtf8(tab.name.data(), static_cast<int>(tab.name.size()));
        m["hasUnsavedChanges"] = tab.has_unsaved;
        out.append(m);
    }
    return out;
}

int WorkspaceBridge::currentIndex() const {
    return neoqtl::workspace_current_index();
}

void WorkspaceBridge::setCurrentIndex(int index) {
    neoqtl::workspace_set_current_index(index);
    Q_EMIT currentIndexChanged();
    Q_EMIT tabsChanged();
    Q_EMIT currentTimelineChanged();
    Q_EMIT currentSceneIdChanged();
}

QObject *WorkspaceBridge::currentTimeline() const {
    return timeline_bridge_instance();
}

int WorkspaceBridge::currentSceneId() const {
    for (const auto &s : neoqtl::timeline_scene_tabs()) {
        if (s.active) {
            return s.id;
        }
    }
    return -1;
}

bool WorkspaceBridge::newProject() {
    const bool ok = neoqtl::workspace_new_project();
    if (ok) {
        Q_EMIT tabsChanged();
        Q_EMIT currentIndexChanged();
    }
    return ok;
}

bool WorkspaceBridge::loadProject(const QUrl &url) {
    const QString path = url.isLocalFile() ? url.toLocalFile() : url.toString();
    const QByteArray utf8 = path.toUtf8();
    const bool ok = neoqtl::workspace_load_project(rust::Str(utf8.constData(), utf8.size()));
    if (ok) {
        Q_EMIT tabsChanged();
        Q_EMIT currentIndexChanged();
    }
    return ok;
}

bool WorkspaceBridge::closeProject(int index) {
    const bool ok = neoqtl::workspace_close_project(index);
    if (ok) {
        Q_EMIT tabsChanged();
        Q_EMIT currentIndexChanged();
    }
    return ok;
}

void register_workspace_bridge(QQmlApplicationEngine &engine) {
    static WorkspaceBridge instance;
    engine.rootContext()->setContextProperty(QStringLiteral("Workspace"), &instance);
}
