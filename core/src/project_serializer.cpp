#include "project_serializer.hpp"
#include "../../ui/include/project_service.hpp"
#include "../../ui/include/timeline_service.hpp"
#include "effect_model.hpp"
#include "effect_registry.hpp"
#include "settings_manager.hpp"
#include <QDebug>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUrl>
#include <algorithm>

namespace AviQtl::Core {

auto ProjectSerializer::save(const QString &fileUrl, const UI::TimelineService *timeline, const UI::ProjectService *project, QString *errorMessage) -> bool {
    QString path = QUrl(fileUrl).toLocalFile();
    if (path.isEmpty()) {
        path = fileUrl;
    }

    QJsonObject root;
    QJsonObject settings;
    settings.insert(QStringLiteral("width"), project->width());
    settings.insert(QStringLiteral("height"), project->height());
    settings.insert(QStringLiteral("fps"), project->fps());
    settings.insert(QStringLiteral("sampleRate"), project->sampleRate());
    root.insert(QStringLiteral("settings"), settings);

    QJsonArray scenesArray;
    for (const auto &scene : timeline->getAllScenes()) {
        QJsonObject sObj;
        sObj.insert(QStringLiteral("id"), scene.id);
        sObj.insert(QStringLiteral("name"), scene.name);
        sObj.insert(QStringLiteral("width"), scene.width);
        sObj.insert(QStringLiteral("height"), scene.height);
        sObj.insert(QStringLiteral("fps"), scene.fps);
        sObj.insert(QStringLiteral("totalFrames"), scene.totalFrames);
        sObj.insert(QStringLiteral("start"), scene.startFrame);
        sObj.insert(QStringLiteral("duration"), scene.durationFrames);
        scenesArray.append(sObj);
    }
    root.insert(QStringLiteral("scenes"), scenesArray);

    QJsonArray clipsArray;
    for (const auto &scene : timeline->getAllScenes()) {
        for (const auto &clip : std::as_const(scene.clips)) {
            QJsonObject clipObj;
            clipObj.insert(QStringLiteral("id"), clip.id);
            clipObj.insert(QStringLiteral("sceneId"), clip.sceneId);
            clipObj.insert(QStringLiteral("type"), clip.type);
            clipObj.insert(QStringLiteral("start"), clip.startFrame);
            clipObj.insert(QStringLiteral("duration"), clip.durationFrames);
            clipObj.insert(QStringLiteral("layer"), clip.layer);
            clipObj.insert(QStringLiteral("params"), QJsonObject::fromVariantMap(clip.params));

            QJsonArray audioPluginsArray;
            for (const auto &plugin : std::as_const(clip.audioPlugins)) {
                QJsonObject pObj;
                pObj.insert(QStringLiteral("id"), plugin.id);
                pObj.insert(QStringLiteral("enabled"), plugin.enabled);
                pObj.insert(QStringLiteral("params"), QJsonObject::fromVariantMap(plugin.params));
                audioPluginsArray.append(pObj);
            }
            clipObj.insert(QStringLiteral("audioPlugins"), audioPluginsArray);

            QJsonArray effArray;
            for (const auto *eff : std::as_const(clip.effects)) {
                QJsonObject eObj;
                eObj.insert(QStringLiteral("id"), eff->id());
                eObj.insert(QStringLiteral("name"), eff->name());
                eObj.insert(QStringLiteral("enabled"), eff->isEnabled());
                eObj.insert(QStringLiteral("params"), QJsonObject::fromVariantMap(eff->params()));
                eObj.insert(QStringLiteral("keyframes"), QJsonObject::fromVariantMap(eff->keyframeTracks()));
                effArray.append(eObj);
            }
            clipObj.insert(QStringLiteral("effects"), effArray);
            clipsArray.append(clipObj);
        }
    }
    root.insert(QStringLiteral("clips"), clipsArray);

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) {
        if (errorMessage != nullptr)
            *errorMessage = file.errorString();
        return false;
    }
    file.write(QJsonDocument(root).toJson());
    return true;
}

auto ProjectSerializer::load(const QString &fileUrl, UI::TimelineService *timeline, UI::ProjectService *project, QString *errorMessage) -> bool {
    QString path = QUrl(fileUrl).toLocalFile();
    if (path.isEmpty())
        path = fileUrl;

    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        if (errorMessage != nullptr)
            *errorMessage = file.errorString();
        return false;
    }

    QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
    QJsonObject root = doc.object();

    QJsonObject s = root.value(QStringLiteral("settings")).toObject();
    project->setWidth(s.value(QStringLiteral("width")).toInt(1920));
    project->setHeight(s.value(QStringLiteral("height")).toInt(1080));
    project->setFps(s.value(QStringLiteral("fps")).toDouble(60.0));
    project->setSampleRate(s.value(QStringLiteral("sampleRate")).toInt(48000));

    QList<UI::SceneData> tempScenes;
    int maxSceneId = 0;
    QJsonArray scenesArray = root.value(QStringLiteral("scenes")).toArray();
    for (const auto &val : std::as_const(scenesArray)) {
        QJsonObject sobj = val.toObject();
        UI::SceneData scene;
        scene.id = sobj.value(QStringLiteral("id")).toInt();
        scene.name = sobj.value(QStringLiteral("name")).toString();
        scene.width = sobj.value(QStringLiteral("width")).toInt(project->width());
        scene.height = sobj.value(QStringLiteral("height")).toInt(project->height());
        scene.fps = sobj.value(QStringLiteral("fps")).toDouble(project->fps());
        scene.totalFrames = sobj.value(QStringLiteral("totalFrames")).toInt(300);
        scene.startFrame = sobj.value(QStringLiteral("start")).toInt(0);
        scene.durationFrames = sobj.value(QStringLiteral("duration")).toInt(0);
        tempScenes.append(scene);
        maxSceneId = std::max(scene.id, maxSceneId);
    }

    QJsonArray clipsArray = root.value(QStringLiteral("clips")).toArray();
    int maxClipId = 0;
    for (const auto &val : std::as_const(clipsArray)) {
        QJsonObject c = val.toObject();
        UI::ClipData clip;
        clip.id = c.value(QStringLiteral("id")).toInt();
        clip.sceneId = c.value(QStringLiteral("sceneId")).toInt(0);
        maxClipId = std::max(clip.id, maxClipId);
        clip.type = c.value(QStringLiteral("type")).toString();
        clip.startFrame = c.value(QStringLiteral("start")).toInt();
        clip.durationFrames = c.value(QStringLiteral("duration")).toInt();
        clip.layer = c.value(QStringLiteral("layer")).toInt();
        clip.params = c.value(QStringLiteral("params")).toObject().toVariantMap();

        QJsonArray audioPluginsArray = c.value(QStringLiteral("audioPlugins")).toArray();
        for (const auto &pv : std::as_const(audioPluginsArray)) {
            QJsonObject pObj = pv.toObject();
            UI::AudioPluginState plugin;
            plugin.id = pObj.value(QStringLiteral("id")).toString();
            plugin.enabled = pObj.value(QStringLiteral("enabled")).toBool(true);
            plugin.params = pObj.value(QStringLiteral("params")).toObject().toVariantMap();
            if (!plugin.id.isEmpty()) {
                clip.audioPlugins.append(plugin);
            }
        }

        QJsonArray effArr = c.value(QStringLiteral("effects")).toArray();
        for (const auto &ev : std::as_const(effArr)) {
            QJsonObject eObj = ev.toObject();
            QString effId = eObj.value(QStringLiteral("id")).toString();
            EffectMetadata meta = EffectRegistry::instance().getEffect(effId);
            QString displayName = meta.name.isEmpty() ? eObj.value(QStringLiteral("name")).toString() : meta.name;
            auto *eff = new UI::EffectModel(effId, displayName, meta.kind, meta.categories, eObj.value(QStringLiteral("params")).toObject().toVariantMap(), meta.qmlSource, meta.uiDefinition, timeline);
            eff->setEnabled(eObj.value(QStringLiteral("enabled")).toBool(true));
            auto it = eObj.find(QStringLiteral("keyframes"));
            if (it != eObj.end()) {
                eff->setKeyframeTracks(it.value().toObject().toVariantMap());
            }
            clip.effects.append(eff);
        }

        for (auto &scene : tempScenes) {
            if (scene.id == clip.sceneId) {
                scene.clips.append(clip);
                break;
            }
        }
    }

    timeline->setScenes(tempScenes);
    timeline->setNextClipId(maxClipId + 1);
    timeline->setNextSceneId(maxSceneId + 1);
    QMetaObject::invokeMethod(timeline, "clipsChanged");

    return true;
}

} // namespace AviQtl::Core
