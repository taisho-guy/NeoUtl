#include "settings_manager.hpp"
#include "timeline_controller.hpp"
#include "timeline_service.hpp"

namespace AviQtl::UI {

void TimelineController::exportVideoAsync(const QVariantMap &cfg) {
    AviQtl::Core::VideoEncoder::Config c;
    c.width = cfg.value(QStringLiteral("width"), 1920).toInt();
    c.height = cfg.value(QStringLiteral("height"), 1080).toInt();
    c.fps_num = cfg.value(QStringLiteral("fps_num"), 60000).toInt();
    c.fps_den = cfg.value(QStringLiteral("fps_den"), 1000).toInt();
    c.bitrate = cfg.value(QStringLiteral("bitrate"), 15'000'000).toLongLong();
    c.crf = cfg.value(QStringLiteral("crf"), -1).toInt();
    c.codecName = cfg.value(QStringLiteral("codecName"), "h264_vaapi").toString();
    c.audioCodecName = cfg.value(QStringLiteral("audioCodecName"), "aac").toString();
    c.audioBitrate = cfg.value(QStringLiteral("audioBitrate"), 192'000).toLongLong();
    c.outputUrl = cfg.value(QStringLiteral("outputUrl")).toString();
    c.startFrame = cfg.value(QStringLiteral("startFrame"), 0).toInt();
    c.endFrame = cfg.value(QStringLiteral("endFrame"), -1).toInt();
    m_exportManager->exportVideoAsync(c);
}

void TimelineController::cancelExport() { m_exportManager->cancelExport(); }
auto TimelineController::isExporting() const -> bool { return m_exportManager->isExporting(); }

} // namespace AviQtl::UI