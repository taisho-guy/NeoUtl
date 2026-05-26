#include "timeline_controller.hpp"
#include "audio_decoder.hpp"
#include "commands.hpp"
#include "core/include/document_model.hpp"
#include "effect_registry.hpp"
#include "engine/plugin/audio_plugin_manager.hpp"
#include "engine/timeline/bake_controller.hpp"
#include "project_serializer.hpp"
#include "project_service.hpp"
#include "scripting/lua_host.hpp"
#include "selection_service.hpp"
#include "settings_manager.hpp"
#include "timeline_service.hpp"
#include "transport_service.hpp"
#include "video_decoder.hpp"
#include "video_frame_store.hpp"
#include <QFile>
#include <QSet>
#include <QSignalBlocker>
#include <QUndoStack>
#include <QUrl>
#include <QtGlobal>
#include <algorithm>

namespace AviQtl::UI {

TimelineController::TimelineController(QObject *parent) : QObject(parent) {
    initializeServices();
    setupConnections();

    // 初期状態の設定
    m_selection->select(-1, QVariantMap());
    syncTimelineToDocumentModel();
    AviQtl::Engine::Timeline::BakeController::instance().bake(currentSceneId(), m_transport->currentFrame());
    updateClipActiveState();
    invalidateTimelineDuration();
    m_transport->setTotalFrames(timelineDuration());
}

void TimelineController::initializeServices() {
    m_project = new ProjectService(this);
    m_transport = new TransportService(this);
    m_selection = new SelectionService(this);
    m_timeline = new TimelineService(m_selection, this);

    m_mediaManager = new TimelineMediaManager(this, this);
    m_exportManager = new TimelineExportManager(this, this);
}

void TimelineController::setupConnections() {
    connect(
        m_timeline, &TimelineService::clipsChanged, this,
        [this]() -> void {
            AviQtl::Engine::Timeline::BakeController::instance().bake(currentSceneId(), m_transport->currentFrame());
            emit clipsChanged();
            m_mediaManager->updateMediaDecoders();
            m_mediaManager->onCurrentFrameChanged();
            updateActiveClipsList();
            invalidateTimelineDuration();
            m_transport->setTotalFrames(timelineDuration());
        },
        Qt::QueuedConnection);

    connect(m_selection, &SelectionService::selectedClipDataChanged, this, [this]() -> void {
        emit clipStartFrameChanged();
        emit clipDurationFramesChanged();
        emit layerChanged();
        emit activeObjectTypeChanged();
        updateClipActiveState();
    });

    connect(m_timeline, &TimelineService::scenesChanged, this, [this]() -> void {
        m_mediaManager->updateMediaDecoders();
        m_mediaManager->onCurrentFrameChanged();
        updateActiveClipsList();
        invalidateTimelineDuration();
        emit scenesChanged();
    });
    connect(m_timeline, &TimelineService::currentSceneIdChanged, this, [this]() {
        invalidateTimelineDuration();
        emit currentSceneIdChanged();
    });
    connect(m_timeline, &TimelineService::clipEffectsChanged, this, [this](int id) -> void {
        m_mediaManager->onCurrentFrameChanged();
        updateActiveClipsList();
        emit clipEffectsChanged(id);
    });
    // 引数付きのシグナルを QML へ転送
    connect(m_timeline, &TimelineService::effectParamChanged, this, &TimelineController::effectParamChanged);
    connect(m_timeline, &TimelineService::effectParamChanged, this, [this]() {
        m_mediaManager->onCurrentFrameChanged();
        updateActiveClipsList();
    });

    // 画像や動画の準備ができたらUI側に再描画を促す
    connect(m_mediaManager, &TimelineMediaManager::frameUpdated, this, &TimelineController::clipEffectsChanged);

    connect(m_exportManager, &TimelineExportManager::exportStarted, this, &TimelineController::exportStarted);
    connect(m_exportManager, &TimelineExportManager::exportProgressChanged, this, &TimelineController::exportProgressChanged);
    connect(m_exportManager, &TimelineExportManager::exportFinished, this, &TimelineController::exportFinished);

    connect(m_project, &ProjectService::fpsChanged, this, [this]() -> void { m_transport->updateTimerInterval(m_project->fps()); });
    m_transport->updateTimerInterval(m_project->fps());

    connect(m_transport, &TransportService::isPlayingChanged, this, &TimelineController::onPlayingChanged);

    // 再生位置が変わったらプレビュー更新
    connect(m_transport, &TransportService::currentFrameChanged, this, &TimelineController::onCurrentFrameChanged);

    // QML(VideoObject)からのフレーム要求をMediaManagerへ中継
    connect(this, &TimelineController::videoFrameRequested, m_mediaManager, &TimelineMediaManager::requestVideoFrame);
    connect(this, &TimelineController::imageLoadRequested, m_mediaManager, &TimelineMediaManager::requestImageLoad);

    connect(m_timeline->undoStack(), &QUndoStack::cleanChanged, this, [this](bool) { emit hasUnsavedChangesChanged(); });
}

void TimelineController::onPlayingChanged() { m_mediaManager->onPlayingChanged(); }

void TimelineController::onCurrentFrameChanged() {
    m_mediaManager->onCurrentFrameChanged();
    updateActiveClipsList();
}

void TimelineController::setVideoFrameStore(AviQtl::Core::VideoFrameStore *store) {
    qDebug() << "TimelineController: VideoFrameStore set. Updating decoders...";
    m_mediaManager->setVideoFrameStore(store);
}

void TimelineController::togglePlay() {
    if (m_transport != nullptr) {
        m_transport->togglePlay();
    }
}

void TimelineController::undo() { m_timeline->undo(); }
void TimelineController::redo() { m_timeline->redo(); }

auto TimelineController::timelineScale() const -> double { return m_timelineScale; }
void TimelineController::setTimelineScale(double scale) {
    if (qAbs(m_timelineScale - scale) > 0.001) {
        m_timelineScale = scale;
        emit timelineScaleChanged();
    }
}

void TimelineController::updateActiveClipsList() {
    syncTimelineToDocumentModel();
    AviQtl::Engine::Timeline::BakeController::instance().bake(currentSceneId(), m_transport->currentFrame());
}

void TimelineController::invalidateTimelineDuration() {
    int oldVal = m_cachedTimelineDuration;
    const auto *scene = AviQtl::Core::DocumentModel::instance().findScene(currentSceneId());
    if (scene) {
        int maxEnd = 0;
        const auto &sceneClips = m_timeline->clips(currentSceneId());
        for (const auto &clip : sceneClips) {
            if (clip.durationFrames > 0) {
                maxEnd = std::max(maxEnd, clip.startFrame + clip.durationFrames);
            }
        }
        m_cachedTimelineDuration = std::max(1, maxEnd);
    } else {
        m_cachedTimelineDuration = 300; // fallback
    }
    if (m_cachedTimelineDuration != oldVal) {
        emit timelineDurationChanged();
    }
}

void TimelineController::log(const QString &msg) { qDebug() << "[TimelineBridge] " << msg; }

auto TimelineController::resolveDragPosition(int clipId, int targetLayer, int proposedStartFrame, const QVariantList &batchIds) -> QPoint { return m_timeline->resolveDragPosition(clipId, targetLayer, proposedStartFrame, batchIds); }

auto TimelineController::resolveDragDelta(int clipId, int deltaFrame, int deltaLayer, const QVariantList &batchIds, int minFrame, int minLayer, int maxLayer, int totalLayers) -> QPoint {
    const auto *clip = m_timeline->findClipById(clipId);
    if (clip == nullptr) {
        return {0, 0};
    }
    QPoint resolved = m_timeline->resolveDragPosition(clipId, clip->layer + deltaLayer, clip->startFrame + deltaFrame, batchIds);

    int dF = resolved.x() - clip->startFrame;
    int dL = resolved.y() - clip->layer;

    if (minFrame + dF < 0) {
        dF = -minFrame;
    }
    if (minLayer + dL < 0) {
        dL = -minLayer;
    }
    if (maxLayer + dL >= totalLayers) {
        dL = totalLayers - 1 - maxLayer;
    }

    return {dF, dL};
}

void TimelineController::requestVideoFrame(int clipId, int relFrame) { emit videoFrameRequested(clipId, relFrame); }

void TimelineController::requestImageLoad(int clipId, const QString &path) { emit imageLoadRequested(clipId, path); }

bool TimelineController::hasUnsavedChanges() const {
    if (m_timeline && m_timeline->undoStack()) {
        return !m_timeline->undoStack()->isClean();
    }
    return false;
}

void TimelineController::syncTimelineToDocumentModel() {
    auto &doc = AviQtl::Core::DocumentModel::instance();
    // 同期中の連続的な信号発火を抑制し、最後に一度だけ Bake を走らせる
    QSignalBlocker blocker(&doc);

    AviQtl::Core::ProjectSettings projSettings;
    if (m_project) {
        projSettings.defaultSceneWidth = m_project->width();
        projSettings.defaultSceneHeight = m_project->height();
        projSettings.defaultFps = m_project->fps();
        projSettings.audioSampleRate = m_project->sampleRate();
    }
    doc.setProjectSettings(projSettings);

    if (!m_timeline)
        return;

    const auto &uiScenes = m_timeline->getAllScenes();
    QSet<int> incomingSceneIds;
    for (const auto &s : uiScenes)
        incomingSceneIds.insert(s.id);

    QSet<int> existingDocSceneIds;
    for (const auto &s : doc.scenes())
        existingDocSceneIds.insert(s.id);

    for (int id : existingDocSceneIds) {
        if (!incomingSceneIds.contains(id))
            doc.removeScene(id);
    }

    for (const auto &uiScene : uiScenes) {
        AviQtl::Core::SceneSettings sceneSettings;
        sceneSettings.id = uiScene.id;
        sceneSettings.name = uiScene.name;
        sceneSettings.width = uiScene.width;
        sceneSettings.height = uiScene.height;
        sceneSettings.fps = uiScene.fps;
        sceneSettings.enableSnap = uiScene.enableSnap;
        sceneSettings.gridMode = uiScene.gridMode;

        // レイヤー状態
        for (int layer : uiScene.lockedLayers) {
            sceneSettings.lockedLayers.push_back(layer);
        }
        for (int layer : uiScene.hiddenLayers) {
            sceneSettings.hiddenLayers.push_back(layer);
        }

        // クリップ
        std::vector<AviQtl::Core::Clip> coreClips;
        for (const auto &uiClip : uiScene.clips) {
            AviQtl::Core::Clip clip;
            clip.id = uiClip.id;
            clip.sceneId = uiClip.sceneId;
            clip.type = uiClip.type;
            clip.layer = uiClip.layer;
            clip.startFrame = uiClip.startFrame;
            clip.durationFrames = uiClip.durationFrames;
            clip.params = uiClip.params;

            // エフェクト
            for (const auto *uiEff : uiClip.effects) {
                if (!uiEff)
                    continue;

                AviQtl::Core::Effect effect;
                effect.id = uiEff->id();
                effect.enabled = uiEff->isEnabled();
                effect.params = uiEff->params();

                // キーフレーム
                QVariantMap tracks = uiEff->keyframeTracks();
                for (auto it = tracks.begin(); it != tracks.end(); ++it) {
                    const QString &paramName = it.key();
                    std::vector<AviQtl::Core::Keyframe> kfs;

                    QVariantList flatPoints = uiEff->keyframeListForUi(paramName);
                    for (const auto &ptVar : std::as_const(flatPoints)) {
                        QVariantMap ptMap = ptVar.toMap();
                        AviQtl::Core::Keyframe kf;
                        kf.frame = ptMap.value(QStringLiteral("frame")).toInt();
                        kf.value = static_cast<float>(ptMap.value(QStringLiteral("value")).toDouble());
                        kf.interpolation = ptMap.value(QStringLiteral("interp"), QStringLiteral("linear")).toString();
                        kf.bzx1 = static_cast<float>(ptMap.value(QStringLiteral("bzx1"), 0.33).toDouble());
                        kf.bzy1 = static_cast<float>(ptMap.value(QStringLiteral("bzy1"), 0.0).toDouble());
                        kf.bzx2 = static_cast<float>(ptMap.value(QStringLiteral("bzx2"), 0.66).toDouble());
                        kf.bzy2 = static_cast<float>(ptMap.value(QStringLiteral("bzy2"), 1.0).toDouble());
                        kf.expression = ptMap.value(QStringLiteral("expression")).toString();
                        kfs.push_back(kf);
                    }

                    effect.keyframes[paramName] = std::move(kfs);
                }

                clip.effects.push_back(std::move(effect));
            }

            coreClips.push_back(std::move(clip));
        }

        if (existingDocSceneIds.contains(uiScene.id)) {
            // 既存シーンの更新
            doc.updateSceneSettings(sceneSettings);
            doc.setClips(uiScene.id, std::move(coreClips));
        } else {
            // 新規シーンの追加
            sceneSettings.clips = std::move(coreClips);
            doc.addScene(sceneSettings);
        }
    }

    blocker.unblock();
    emit doc.structureChanged();
}

} // namespace AviQtl::UI
