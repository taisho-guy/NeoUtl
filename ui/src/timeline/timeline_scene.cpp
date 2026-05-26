#include "commands.hpp"
#include "selection_service.hpp"
#include "settings_manager.hpp"
#include "timeline_service.hpp"
#include <QDebug>

namespace AviQtl::UI {

auto TimelineService::currentScene() -> SceneData * {
    for (auto &scene : m_scenes) {
        if (scene.id == m_currentSceneId) {
            return &scene;
        }
    }
    if (m_scenes.isEmpty()) {
        static SceneData dummy;
        return &dummy;
    }
    return m_scenes.data();
}

auto TimelineService::currentScene() const -> const SceneData * {
    for (const auto &scene : std::as_const(m_scenes)) {
        if (scene.id == m_currentSceneId) {
            return &scene;
        }
    }
    if (m_scenes.isEmpty()) {
        static SceneData dummy;
        return &dummy;
    }
    return m_scenes.data();
}

auto TimelineService::scenes() const -> QVariantList {
    QVariantList list;
    for (const auto &scene : std::as_const(m_scenes)) {
        QVariantMap map;
        map.insert(QStringLiteral("id"), scene.id);
        map.insert(QStringLiteral("name"), scene.name);
        map.insert(QStringLiteral("width"), scene.width);
        map.insert(QStringLiteral("height"), scene.height);
        map.insert(QStringLiteral("fps"), scene.fps);
        map.insert(QStringLiteral("totalFrames"), scene.totalFrames);
        map.insert(QStringLiteral("gridMode"), scene.gridMode);
        map.insert(QStringLiteral("gridBpm"), scene.gridBpm);
        map.insert(QStringLiteral("gridOffset"), scene.gridOffset);
        map.insert(QStringLiteral("gridInterval"), scene.gridInterval);
        map.insert(QStringLiteral("gridSubdivision"), scene.gridSubdivision);
        map.insert(QStringLiteral("enableSnap"), scene.enableSnap);
        map.insert(QStringLiteral("magneticSnapRange"), scene.magneticSnapRange);
        list.append(map);
    }
    return list;
}

void TimelineService::setScenes(const QList<SceneData> &scenes) {
    for (auto &scene : m_scenes) {
        for (auto &clip : scene.clips) {
            for (auto *eff : std::as_const(clip.effects)) {
                if (eff)
                    eff->deleteLater();
            }
            clip.effects.clear();
        }
    }

    m_scenes = scenes;
    if (m_scenes.isEmpty()) {
        createScene(QObject::tr("ルート"));
    }
    // 現在のシーンIDが有効か確認
    bool found = false;
    for (const auto &s : std::as_const(m_scenes)) {
        if (s.id == m_currentSceneId) {
            found = true;
            break;
        }
    }
    if (!found && !m_scenes.isEmpty()) {
        m_currentSceneId = m_scenes.first().id;
    }
    emit scenesChanged();
    emit currentSceneIdChanged();
    emit clipsChanged();
}

void TimelineService::createScene(const QString &name) {
    int id = m_nextSceneId++;
    m_undoStack->push(new AddSceneCommand(this, id, name));
}

void TimelineService::removeScene(int sceneId) {
    for (const auto &s : getAllScenes()) {
        if (s.id == sceneId) {
            m_undoStack->push(new RemoveSceneCommand(this, sceneId, s.name));
            return;
        }
    }
}

void TimelineService::switchScene(int sceneId) {
    if (m_currentSceneId == sceneId) {
        return;
    }

    bool exists = false;
    for (const auto &s : std::as_const(m_scenes)) {
        if (s.id == sceneId) {
            exists = true;
            break;
        }
    }
    if (!exists) {
        return;
    }

    m_currentSceneId = sceneId;
    emit currentSceneIdChanged();
    emit clipsChanged();

    if (m_selection != nullptr) {
        m_selection->select(-1, QVariantMap());
    }
}

void TimelineService::updateSceneSettings(int sceneId, const QString &name, int width, int height, double fps, int totalFrames, const QString &gridMode, double gridBpm, double gridOffset, int gridInterval, int gridSubdivision,
                                          bool enableSnap, // NOLINT(bugprone-easily-swappable-parameters)
                                          int magneticSnapRange) {
    SceneData newData;
    SceneData oldData;
    for (const auto &s : getAllScenes()) {
        if (s.id == sceneId) {
            oldData = s;
            break;
        }
    }
    newData = oldData;
    newData.name = name;
    newData.width = width;
    newData.height = height;
    newData.fps = fps;
    newData.totalFrames = totalFrames;
    newData.gridMode = gridMode;
    newData.gridBpm = gridBpm;
    newData.gridOffset = gridOffset;
    newData.gridInterval = gridInterval;
    newData.gridSubdivision = gridSubdivision;
    newData.enableSnap = enableSnap;
    newData.magneticSnapRange = magneticSnapRange;
    m_undoStack->push(new UpdateSceneSettingsCommand(this, sceneId, oldData, newData));
}

void TimelineService::createSceneInternal(int sceneId, const QString &name) {
    SceneData newScene;
    newScene.id = sceneId;
    newScene.name = name;
    const auto &settings = AviQtl::Core::SettingsManager::instance().settings();
    newScene.width = settings.value(QStringLiteral("defaultProjectWidth"), 1920).toInt();
    newScene.height = settings.value(QStringLiteral("defaultProjectHeight"), 1080).toInt();
    newScene.fps = settings.value(QStringLiteral("defaultProjectFps"), 60.0).toDouble();
    m_scenes.append(newScene);
    emit scenesChanged();
    switchScene(newScene.id);
}

void TimelineService::removeSceneInternal(int sceneId) {
    if (sceneId == 0) {
        return;
    }
    auto it = std::ranges::find_if(m_scenes, [sceneId](const SceneData &s) -> bool { return s.id == sceneId; });
    if (it != m_scenes.end()) {
        for (auto &clip : it->clips) {
            for (auto *eff : std::as_const(clip.effects)) {
                eff->deleteLater();
            }
        }
        if (m_currentSceneId == sceneId) {
            switchScene(0);
        }
        m_scenes.erase(it);
        emit scenesChanged();
    }
}

void TimelineService::restoreSceneInternal(const SceneData &scene) {
    m_scenes.append(scene);
    emit scenesChanged();
}

void TimelineService::applySceneSettingsInternal(int sceneId, const SceneData &data) {
    for (auto &scene : m_scenes) {
        if (scene.id == sceneId) {
            scene.name = data.name;
            scene.width = data.width;
            scene.height = data.height;
            scene.fps = data.fps;
            scene.totalFrames = data.totalFrames;
            scene.gridMode = data.gridMode;
            scene.gridBpm = data.gridBpm;
            scene.gridOffset = data.gridOffset;
            scene.gridInterval = data.gridInterval;
            scene.gridSubdivision = data.gridSubdivision;
            scene.enableSnap = data.enableSnap;
            scene.magneticSnapRange = data.magneticSnapRange;
            emit scenesChanged();
            return;
        }
    }
}

} // namespace AviQtl::UI