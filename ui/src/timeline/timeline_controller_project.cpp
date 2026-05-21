#include "effect_registry.hpp"
#include "engine/plugin/audio_plugin_manager.hpp"
#include "project_serializer.hpp"
#include "project_service.hpp"
#include "settings_manager.hpp"
#include "timeline_controller.hpp"
#include "timeline_service.hpp"
#include <QFile>
#include <QFileInfo>
#include <QUrl>

namespace AviQtl::UI {

namespace {
void addRecentProject(const QString &fileUrl, ProjectService *project) {
    if (fileUrl.isEmpty() || !project)
        return;

    QString path = QUrl(fileUrl).toLocalFile();
    if (path.isEmpty()) {
        path = fileUrl;
    }
    QString name = QFileInfo(path).fileName();

    auto &settingsManager = AviQtl::Core::SettingsManager::instance();
    QVariantList recentList = settingsManager.value(QStringLiteral("recentProjects"), QVariantList()).toList();

    // 重複を避けて先頭に追加する
    QVariantList newList;
    QVariantMap newEntry;
    newEntry[QStringLiteral("name")] = name;
    newEntry[QStringLiteral("path")] = path;
    newEntry[QStringLiteral("width")] = project->width();
    newEntry[QStringLiteral("height")] = project->height();
    newEntry[QStringLiteral("fps")] = project->fps();

    newList.append(newEntry);

    for (const auto &val : recentList) {
        QVariantMap entry = val.toMap();
        if (entry.value(QStringLiteral("path")).toString() != path) {
            newList.append(entry);
        }
    }

    // 最大件数でトリミング
    int maxRecent = settingsManager.value(QStringLiteral("recentProjectMaxCount"), 10).toInt();
    while (newList.size() > maxRecent) {
        newList.removeLast();
    }

    settingsManager.setValue(QStringLiteral("recentProjects"), newList);
    settingsManager.save();
}
} // namespace

auto TimelineController::saveProject(const QString &fileUrl) -> bool {
    // 渡されたパスが空の場合は内部で保持しているパスを割り当てる
    QString targetUrl = fileUrl.isEmpty() ? m_currentProjectUrl : fileUrl;

    // パスが空の場合は新規作成直後なのでエラーで返す
    if (targetUrl.isEmpty()) {
        emit errorOccurred(tr("保存先のファイルパスが不明です"));
        return false;
    }

    QString error;
    bool result = AviQtl::Core::ProjectSerializer::save(targetUrl, m_timeline, m_project, &error);

    if (result) {
        // 保存に成功したパスを現在のプロジェクトパスとして記憶する
        m_currentProjectUrl = targetUrl;
        m_timeline->undoStack()->setClean();
        emit currentProjectUrlChanged();
        emit hasUnsavedChangesChanged();
        addRecentProject(targetUrl, m_project);
    } else {
        emit errorOccurred(error);
    }
    return result;
}

auto TimelineController::loadProject(const QString &fileUrl) -> bool {
    QString error;
    bool result = AviQtl::Core::ProjectSerializer::load(fileUrl, m_timeline, m_project, &error);

    if (result) {
        // 読み込みに成功したパスを現在のプロジェクトパスとして記憶する
        m_currentProjectUrl = fileUrl;
        m_timeline->undoStack()->setClean();
        emit currentProjectUrlChanged();
        emit hasUnsavedChangesChanged();
        addRecentProject(fileUrl, m_project);
    } else {
        emit errorOccurred(error);
    }
    return result;
}


namespace {
void insertIntoCategoryTree(QVariantList &list, const QStringList &path, const QVariantMap &item) {
    if (path.isEmpty()) {
        list.append(item);
        return;
    }

    QString currentCategory = path.first();
    int foundIdx = -1;
    for (int i = 0; i < list.size(); ++i) {
        QVariantMap node = list[i].toMap();
        if (node.value(QStringLiteral("isCategory")).toBool() && node.value(QStringLiteral("title")).toString() == currentCategory) {
            foundIdx = i;
            break;
        }
    }

    QVariantMap categoryNode;
    QVariantList children;
    if (foundIdx >= 0) {
        categoryNode = list[foundIdx].toMap();
        children = categoryNode.value(QStringLiteral("children")).toList();
    } else {
        categoryNode.insert(QStringLiteral("isCategory"), true);
        categoryNode.insert(QStringLiteral("title"), currentCategory);
    }

    insertIntoCategoryTree(children, path.mid(1), item);
    categoryNode.insert(QStringLiteral("children"), children);

    if (foundIdx >= 0) {
        list[foundIdx] = categoryNode;
    } else {
        list.append(categoryNode);
    }
}
} // namespace

auto TimelineController::getAvailableEffects() -> QVariantList {
    QVariantList list;
    const auto effects = AviQtl::Core::EffectRegistry::instance().getAllEffects();
    for (const auto &meta : effects) {
        if (meta.kind != "effect") {
            continue;
        }
        QVariantMap m;
        m.insert(QStringLiteral("id"), meta.id);
        m.insert(QStringLiteral("name"), meta.name);

        for (const QString &categoryPath : meta.categories) {
            QStringList pathTokens = categoryPath.split(QStringLiteral("/"), Qt::SkipEmptyParts);
            insertIntoCategoryTree(list, pathTokens, m);
        }
    }
    return list;
}

auto TimelineController::getAvailableObjects() -> QVariantList {
    QVariantList list;
    const auto effects = AviQtl::Core::EffectRegistry::instance().getAllEffects();
    for (const auto &meta : effects) {
        if (meta.kind != "object") {
            continue;
        }
        QVariantMap m;
        m.insert(QStringLiteral("id"), meta.id);
        m.insert(QStringLiteral("name"), meta.name);

        for (const QString &categoryPath : meta.categories) {
            QStringList pathTokens = categoryPath.split(QStringLiteral("/"), Qt::SkipEmptyParts);
            insertIntoCategoryTree(list, pathTokens, m);
        }
    }
    return list;
}

auto TimelineController::getClipTypeColor(const QString &type) -> QString { return AviQtl::Core::EffectRegistry::instance().getEffect(type).color; }

auto TimelineController::getAvailableAudioPlugins() -> QVariantList { return AviQtl::Engine::Plugin::AudioPluginManager::instance().getPluginList(); }

auto TimelineController::getPluginCategories() -> QVariantList {
    // AudioPluginManagerから重複のないカテゴリ名リストを抽出
    return AviQtl::Engine::Plugin::AudioPluginManager::instance().getCategories();
}

auto TimelineController::getPluginsByCategory(const QString &category) -> QVariantList {
    // 特定カテゴリに属するプラグインのみを返す
    return AviQtl::Engine::Plugin::AudioPluginManager::instance().getPluginsInCategory(category);
}

} // namespace AviQtl::UI