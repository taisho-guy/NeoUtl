#include "timeline_service.hpp"
#include "commands.hpp"
#include "effect_registry.hpp"
#include "selection_service.hpp"
#include "settings_manager.hpp"
#include <QDebug>
#include <QPoint>
#include <algorithm>
#include <utility>

namespace AviQtl::UI {

TimelineService::TimelineService(SelectionService *selection, QObject *parent) : QObject(parent), m_undoStack(new QUndoStack(this)), m_selection(selection) {
    // 初期シーンを作成
    SceneData rootScene;
    rootScene.id = 0;
    rootScene.name = QObject::tr("ルート");
    const auto &settings = AviQtl::Core::SettingsManager::instance().settings();
    rootScene.width = settings.value(QStringLiteral("defaultProjectWidth"), 1920).toInt();
    rootScene.height = settings.value(QStringLiteral("defaultProjectHeight"), 1080).toInt();
    rootScene.fps = settings.value(QStringLiteral("defaultProjectFps"), 60.0).toDouble();
    m_scenes.append(rootScene);
}

TimelineService::~TimelineService() {
    for (auto &scene : m_scenes) {
        for (auto &clip : scene.clips) {
            for (auto *eff : clip.effects) {
                if (eff)
                    eff->deleteLater();
            }
        }
    }
    for (auto &clip : m_clipboard) {
        for (auto *eff : clip.effects) {
            if (eff)
                eff->deleteLater();
        }
    }
}

void TimelineService::undo() { m_undoStack->undo(); }
void TimelineService::redo() { m_undoStack->redo(); }

} // namespace AviQtl::UI