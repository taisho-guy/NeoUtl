#include "timeline_export_manager.hpp"
#include "engine/audio_mixer.hpp"
#include "settings_manager.hpp"
#include "timeline_controller.hpp"
#include "video_encoder.hpp"
#include <QCoreApplication>
#include <QEventLoop>
#include <QQuickItem>
#include <QQuickItemGrabResult>
#include <QTimer>

namespace AviQtl::UI {

TimelineExportManager::TimelineExportManager(TimelineController *controller, QObject *parent) : QObject(parent), m_controller(controller) {}

TimelineExportManager::~TimelineExportManager() {
    if (m_exportThread) {
        m_cancelRequested = true;
        m_exportThread->wait();
    }
}

void TimelineExportManager::exportVideoAsync(const AviQtl::Core::VideoEncoder::Config &config) {
    if (m_exporting.load()) {
        return;
    }
    m_cancelRequested = false;

    m_exportThread = QThread::create([this, config]() -> void { runExport(config); });
    connect(m_exportThread, &QThread::finished, m_exportThread, &QObject::deleteLater);
    m_exportThread->start();
}

void TimelineExportManager::cancelExport() { m_cancelRequested = true; }

void TimelineExportManager::runExport(const AviQtl::Core::VideoEncoder::Config &config) {
    m_exporting = true;

    AviQtl::Core::VideoEncoder encoder;
    if (!encoder.open(config)) {
        emit exportFinished(false, tr("エンコーダーの初期化に失敗しました"));
        m_exporting = false;
        return;
    }
    const int sr = AviQtl::Core::SettingsManager::instance().value(QStringLiteral("defaultProjectSampleRate"), 48000).toInt();
    const int ch = AviQtl::Core::SettingsManager::instance().value(QStringLiteral("audioChannels"), 2).toInt();
    encoder.addAudioStream(sr, ch);

    const double fps = m_controller->project()->fps();
    const int startFrame = config.startFrame;
    const int endFrame = config.endFrame >= 0 ? config.endFrame : m_controller->timelineDuration();
    const int totalFrames = endFrame - startFrame;

    emit exportStarted(totalFrames);

    QQuickItem *view = m_controller->compositeView();
    QQuickItem *targetItem = (view != nullptr) ? ((view->property("view3D").value<QQuickItem *>() != nullptr) ? view->property("view3D").value<QQuickItem *>() : view) : nullptr;

    if (view != nullptr) {
        QMetaObject::invokeMethod(view, [view] -> void { view->setProperty("exportMode", true); }, Qt::BlockingQueuedConnection);
    }

    auto &exportSettings = AviQtl::Core::SettingsManager::instance();
    const int grabTimeout = exportSettings.value(QStringLiteral("exportFrameGrabTimeoutMs"), 2000).toInt();
    const int progInterval = exportSettings.value(QStringLiteral("exportProgressInterval"), 5).toInt();

    for (int frame = startFrame; frame < endFrame; ++frame) {
        if (m_cancelRequested.load()) {
            emit exportFinished(false, tr("キャンセルされました"));
            goto cleanup;
        }

        QMetaObject::invokeMethod(m_controller->transport(), [this, frame] -> void { m_controller->transport()->setCurrentFrame(frame); }, Qt::BlockingQueuedConnection);

        QImage img;
        if (targetItem != nullptr) {
            QSharedPointer<QQuickItemGrabResult> grab;
            QMetaObject::invokeMethod(targetItem, [&] -> void { grab = targetItem->grabToImage(QSize(config.width, config.height)); }, Qt::BlockingQueuedConnection);
            if (grab) {
                QEventLoop loop;
                connect(grab.get(), &QQuickItemGrabResult::ready, &loop, &QEventLoop::quit);
                QTimer::singleShot(grabTimeout, &loop, &QEventLoop::quit);
                loop.exec();
                img = grab->image();
            }
        }

        encoder.pushFrame(img, frame - startFrame);

        const int samplesNeeded = static_cast<int>(48000 / fps);
        auto audio = m_controller->mediaManager()->audioMixer()->mix(frame, fps, samplesNeeded);
        encoder.pushAudio(audio.data(), samplesNeeded);

        const int done = frame - startFrame + 1;
        if (done % progInterval == 0 || done == totalFrames) {
            emit exportProgressChanged(done * 100 / totalFrames, done, totalFrames);
        }
    }

    encoder.close();
    emit exportFinished(true, tr("書き出し完了"));

cleanup:
    if (view != nullptr) {
        QMetaObject::invokeMethod(view, [view] -> void { view->setProperty("exportMode", false); }, Qt::BlockingQueuedConnection);
    }
    m_exporting = false;
}

} // namespace AviQtl::UI
