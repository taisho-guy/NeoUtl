#pragma once
#include "../../core/include/video_encoder.hpp"
#include <QObject>
#include <QPointer>
#include <QString>
#include <QThread>
#include <atomic>

namespace AviQtl::Core {
class VideoEncoder;
}

namespace AviQtl::UI {
class TimelineController;

class TimelineExportManager : public QObject {
    Q_OBJECT
  public:
    explicit TimelineExportManager(TimelineController *controller, QObject *parent = nullptr);

    ~TimelineExportManager() override;

    void exportVideoAsync(const AviQtl::Core::VideoEncoder::Config &config);
    void cancelExport();
    bool isExporting() const { return m_exporting.load(); }

  signals:
    void exportStarted(int totalFrames);
    void exportProgressChanged(int progress, int currentFrame, int totalFrames);
    void exportFinished(bool success, const QString &message);

  private:
    void runExport(const AviQtl::Core::VideoEncoder::Config &config);
    bool exportImageSequence(const QString &dir, int quality);
    TimelineController *m_controller;
    QPointer<QThread> m_exportThread;
    std::atomic<bool> m_exporting{false};
    std::atomic<bool> m_cancelRequested{false};
};
} // namespace AviQtl::UI