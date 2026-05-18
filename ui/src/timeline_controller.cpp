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
    AviQtl::Engine::Timeline::BakeController::instance().bake(currentSceneId(), m_transport->currentFrame());
    updateClipActiveState();
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
        emit scenesChanged();
    });
    connect(m_timeline, &TimelineService::currentSceneIdChanged, this, &TimelineController::currentSceneIdChanged);
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

    // FPSが変更されたら再生タイマーの間隔を更新
    connect(m_project, &ProjectService::fpsChanged, this, [this]() -> void { m_transport->updateTimerInterval(m_project->fps()); });
    m_transport->updateTimerInterval(m_project->fps());

    // 再生状態の変化をデコーダーに伝播
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
    [[maybe_unused]] int nextFrame = m_transport->currentFrame();
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

void TimelineController::updateActiveClipsList() { AviQtl::Engine::Timeline::BakeController::instance().bake(currentSceneId(), m_transport->currentFrame()); }

int TimelineController::timelineDuration() const {
    const auto *scene = AviQtl::Core::DocumentModel::instance().findScene(currentSceneId());
    if (scene) {
        return scene->totalFrames;
    }
    return 300;
}

void TimelineController::log(const QString &msg) { qDebug() << "[TimelineBridge] " << msg; }

auto TimelineController::resolveDragPosition(int clipId, int targetLayer, int proposedStartFrame, const QVariantList &batchIds) -> QPoint { return m_timeline->resolveDragPosition(clipId, targetLayer, proposedStartFrame, batchIds); }

auto TimelineController::resolveDragDelta(int clipId, int deltaFrame, int deltaLayer, const QVariantList &batchIds, int minFrame, int minLayer, int maxLayer, int totalLayers) -> QPoint {
    const auto *clip = m_timeline->findClipById(clipId);
    if (clip == nullptr) {
        return {0, 0};
    }
    // NOLINT(bugprone-easily-swappable-parameters)
    // 1. 衝突判定を含めた座標解決
    QPoint resolved = m_timeline->resolveDragPosition(clipId, clip->layer + deltaLayer, clip->startFrame + deltaFrame, batchIds);

    int dF = resolved.x() - clip->startFrame;
    int dL = resolved.y() - clip->layer;

    // 2. タイムライン境界によるクランプ (QMLから移行)
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

auto TimelineController::debugRunLua(const QString &script) -> QString {
    // テスト用に time=currentFrame/fps, index=0, value=0 で実行
    double time = (m_transport != nullptr) ? m_transport->currentFrame() / m_project->fps() : 0.0;
    double result = AviQtl::Scripting::LuaHost::instance().evaluate(script.toStdString(), time, 0, 0.0);
    return QString::number(result);
}

void TimelineController::requestVideoFrame(int clipId, int relFrame) {
    // MediaManagerは直接触れないので、TimelineService側にイベントを発火させる等するか、
    // MediaManagerに直接シグナルで飛ばす。
    // ここでは一番手っ取り早い「シグナル」を追加してMediaManagerに拾わせる。
    emit videoFrameRequested(clipId, relFrame);
}

void TimelineController::requestImageLoad(int clipId, const QString &path) { emit imageLoadRequested(clipId, path); }

bool TimelineController::hasUnsavedChanges() const {
    if (m_timeline && m_timeline->undoStack()) {
        return !m_timeline->undoStack()->isClean();
    }
    return false;
}

} // namespace AviQtl::UI
