#include "video_frame_store.hpp"
#include <QDebug>
#include <QMetaObject>
#include <QPointer>
#include <QThread>

namespace AviQtl::Core {

VideoFrameStore::VideoFrameStore(QObject *parent) : QObject(parent) {}

void VideoFrameStore::setFrame(const QString &key, const QImage &frame) {
    QMutexLocker locker(&m_mutex);
    m_frames.insert(key, frame);
}

void VideoFrameStore::setFrameSafe(const QString &key, const QImage &frame) {
    setFrame(key, frame);
    emit frameUpdated(key);
}

void VideoFrameStore::setVideoFrameSafe(const QString &key, const QVideoFrame &frame) {
    if (QThread::currentThread() != thread()) {
        const QVideoFrame &copy(frame);
        QMetaObject::invokeMethod(this, [this, key, copy]() mutable -> void { setVideoFrameSafe(key, copy); }, Qt::QueuedConnection);
        return;
    }

    qDebug() << "[Debug] VideoFrameStore: setVideoFrameSafe called for key:" << key << "valid:" << frame.isValid() << "size:" << frame.size();

    QPointer<QVideoSink> sink;
    {
        QMutexLocker locker(&m_mutex);
        m_lastVideoFrames.insert(key, frame);

        auto it = m_sinks.find(key);
        if (it != m_sinks.end() && !it.value().isNull()) {
            sink = it.value();
        }
    }

    if (sink && frame.isValid()) {
        sink->setVideoFrame(frame);
    }
    emit frameUpdated(key);
}

void VideoFrameStore::registerSink(const QString &key, QVideoSink *sink) {
    if (QThread::currentThread() != thread()) {
        QMetaObject::invokeMethod(this, [this, key, sink]() -> void { registerSink(key, sink); }, Qt::QueuedConnection);
        return;
    }

    QVideoFrame last;
    {
        QMutexLocker locker(&m_mutex);

        auto it = m_sinks.find(key);
        if (it != m_sinks.end() && !it.value().isNull() && it.value()->parent() == this) {
            it.value()->deleteLater();
        }

        m_sinks.insert(key, sink);

        if (sink != nullptr) {
            QObject::connect(sink, &QObject::destroyed, this, [self = QPointer<VideoFrameStore>(this), key, rawSink = sink]() -> void {
                if (!self) {
                    return;
                }
                QMutexLocker locker(&self->m_mutex);
                auto sinkIt = self->m_sinks.find(key);
                if (sinkIt != self->m_sinks.end()) {
                    QVideoSink *current = sinkIt.value().data();
                    if (current == nullptr || current == rawSink) {
                        self->m_sinks.erase(sinkIt);
                    }
                }
            });
        }

        auto lastIt = m_lastVideoFrames.find(key);
        if (lastIt != m_lastVideoFrames.end()) {
            last = lastIt.value();
        }
    }

    if ((sink != nullptr) && last.isValid()) {
        sink->setVideoFrame(last);
        emit frameUpdated(key);
    }
}

auto VideoFrameStore::hasFrame(const QString &key) const -> bool {
    QMutexLocker locker(&m_mutex);
    return m_frames.contains(key) || m_sinks.contains(key);
}

void VideoFrameStore::invalidateFrame(const QString &key) {
    if (QThread::currentThread() != thread()) {
        QMetaObject::invokeMethod(this, [this, key]() -> void { invalidateFrame(key); }, Qt::QueuedConnection);
        return;
    }
    QMutexLocker locker(&m_mutex);
    m_frames.remove(key);
    m_lastVideoFrames.remove(key);
    // Note: QVideoSink 自体は削除しない（QML側で再利用されるため）
}

void VideoFrameStore::invalidateScene(int sceneId) {
    if (QThread::currentThread() != thread()) {
        QMetaObject::invokeMethod(this, [this, sceneId]() -> void { invalidateScene(sceneId); }, Qt::QueuedConnection);
        return;
    }

    QMutexLocker locker(&m_mutex);
    QString prefix = QString::number(sceneId) + QLatin1String("_");

    for (auto it = m_frames.begin(); it != m_frames.end();) {
        if (it.key().startsWith(prefix)) {
            it = m_frames.erase(it);
        } else {
            ++it;
        }
    }

    for (auto it = m_lastVideoFrames.begin(); it != m_lastVideoFrames.end();) {
        if (it.key().startsWith(prefix)) {
            it = m_lastVideoFrames.erase(it);
        } else {
            ++it;
        }
    }
}

void VideoFrameStore::clear() {
    if (QThread::currentThread() != thread()) {
        QMetaObject::invokeMethod(this, [this]() -> void { clear(); }, Qt::QueuedConnection);
        return;
    }
    QMutexLocker locker(&m_mutex);
    m_frames.clear();
    m_lastVideoFrames.clear();
}

auto VideoFrameStore::frame(const QString &key) const -> QImage {
    QMutexLocker locker(&m_mutex);
    return m_frames.value(key);
}

} // namespace AviQtl::Core
