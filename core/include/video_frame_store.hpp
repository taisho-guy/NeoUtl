#pragma once

#include <QHash>
#include <QImage>
#include <QMutex>
#include <QObject>
#include <QPointer>
#include <QVideoFrame>
#include <QVideoSink>

namespace AviQtl::Core {

class VideoFrameStore : public QObject {
    Q_OBJECT
  public:
    explicit VideoFrameStore(QObject *parent = nullptr);

    Q_INVOKABLE void setFrame(const QString &key, const QImage &img);
    Q_INVOKABLE void setFrameSafe(const QString &key, const QImage &img);
    Q_INVOKABLE bool hasFrame(const QString &key) const;
    Q_INVOKABLE void invalidateFrame(const QString &key);
    Q_INVOKABLE void invalidateScene(int sceneId);
    Q_INVOKABLE void clear();

    QImage frame(const QString &key) const;

    // GPU Zero-copy 用
    Q_INVOKABLE void setVideoFrameSafe(const QString &key, const QVideoFrame &frame);
    Q_INVOKABLE void registerSink(const QString &key, QVideoSink *sink);

  signals:
    void frameUpdated(const QString &key);

  private:
    mutable QMutex m_mutex;
    QHash<QString, QImage> m_frames;
    QHash<QString, QVideoFrame> m_lastVideoFrames;
    QHash<QString, QPointer<QVideoSink>> m_sinks;
};

} // namespace AviQtl::Core
