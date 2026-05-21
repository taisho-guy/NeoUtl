#include "document_model.hpp"
#include <algorithm>

namespace AviQtl::Core {

DocumentModel &DocumentModel::instance() {
    static DocumentModel inst;
    return inst;
}

void DocumentModel::clear() {
    m_scenes.clear();
    m_undoStack.clear();
    emit structureChanged();
}

void DocumentModel::setProjectSettings(const ProjectSettings &settings) {
    m_projectSettings = settings;
    // 設定変更自体は構造変化ではないが、プロジェクト再読み込み時は clear 経由で structureChanged が呼ばれる
}

const SceneSettings *DocumentModel::findScene(int sceneId) const {
    auto it = std::find_if(m_scenes.begin(), m_scenes.end(), [sceneId](const SceneSettings &s) { return s.id == sceneId; });
    return (it != m_scenes.end()) ? &(*it) : nullptr;
}

void DocumentModel::addScene(const SceneSettings &scene) {
    m_scenes.push_back(scene);
    emit structureChanged();
}

void DocumentModel::removeScene(int sceneId) {
    auto it = std::remove_if(m_scenes.begin(), m_scenes.end(), [sceneId](const SceneSettings &s) { return s.id == sceneId; });
    if (it != m_scenes.end()) {
        m_scenes.erase(it, m_scenes.end());
        emit structureChanged();
    }
}

const Clip *DocumentModel::findClip(int sceneId, int clipId) const {
    const SceneSettings *scene = findScene(sceneId);
    if (!scene)
        return nullptr;

    auto it = std::find_if(scene->clips.begin(), scene->clips.end(), [clipId](const Clip &c) { return c.id == clipId; });
    return (it != scene->clips.end()) ? &(*it) : nullptr;
}

void DocumentModel::addClip(int sceneId, const Clip &clip) {
    auto it = std::find_if(m_scenes.begin(), m_scenes.end(), [sceneId](const SceneSettings &s) { return s.id == sceneId; });
    if (it != m_scenes.end()) {
        it->clips.push_back(clip);
        emit structureChanged();
    }
}

void DocumentModel::removeClip(int sceneId, int clipId) {
    auto itScene = std::find_if(m_scenes.begin(), m_scenes.end(), [sceneId](const SceneSettings &s) { return s.id == sceneId; });
    if (itScene != m_scenes.end()) {
        auto itClip = std::remove_if(itScene->clips.begin(), itScene->clips.end(), [clipId](const Clip &c) { return c.id == clipId; });
        if (itClip != itScene->clips.end()) {
            itScene->clips.erase(itClip, itScene->clips.end());
            emit structureChanged();
        }
    }
}

} // namespace AviQtl::Core
