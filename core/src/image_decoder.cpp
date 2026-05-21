#include "image_decoder.hpp"
#include "ffmpeg_video_buffer.hpp"
#include "video_frame_store.hpp"
#include <QDebug>
#include <QVideoFrame>
#include <QVideoFrameFormat>
#include <QtConcurrent>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
}

namespace AviQtl::Core {

ImageDecoder::ImageDecoder(int clipId, const QUrl &source, VideoFrameStore *store, QObject *parent) : MediaDecoder(clipId, source, parent), m_store(store) {}

ImageDecoder::~ImageDecoder() {
    if (m_future.isRunning()) {
        m_future.waitForFinished();
    }
}

void ImageDecoder::seek(qint64 ms) {
    Q_UNUSED(ms);
    if (m_isReady && m_cachedVideoFrame.isValid()) {
        // すでにデコード済みの場合は、ストアに再通知してSink（画面）を更新する
        m_store->setVideoFrameSafe(QString::number(clipId()), m_cachedVideoFrame);
    } else if (!m_future.isRunning()) {
        load();
    }
}
void ImageDecoder::setPlaying(bool playing) { Q_UNUSED(playing); }
void ImageDecoder::startDecoding() { load(); }

void ImageDecoder::load() {
    QString path = m_source.toLocalFile();
    if (path.isEmpty()) {
        path = m_source.toString();
    }
    if (path.isEmpty()) {
        return;
    }
    if (m_future.isRunning()) {
        m_future.waitForFinished();
    }
    m_future = QtConcurrent::run([this, path]() -> void { decodeImage(path); });
}

void ImageDecoder::decodeImage(const QString &path) {
    AVFormatContext *fmtCtx = nullptr;
    if (avformat_open_input(&fmtCtx, path.toStdString().c_str(), nullptr, nullptr) != 0) {
        qWarning() << "[ImageDecoder] avformat_open_input失敗:" << path;
        return;
    }
    if (avformat_find_stream_info(fmtCtx, nullptr) < 0) {
        avformat_close_input(&fmtCtx);
        return;
    }

    int streamIdx = av_find_best_stream(fmtCtx, AVMEDIA_TYPE_VIDEO, -1, -1, nullptr, 0);
    if (streamIdx < 0) {
        avformat_close_input(&fmtCtx);
        return;
    }

    AVStream *stream = fmtCtx->streams[streamIdx];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (codec == nullptr) {
        avformat_close_input(&fmtCtx);
        return;
    }

    AVCodecContext *decCtx = avcodec_alloc_context3(codec);
    avcodec_parameters_to_context(decCtx, stream->codecpar);
    if (avcodec_open2(decCtx, codec, nullptr) < 0) {
        avcodec_free_context(&decCtx);
        avformat_close_input(&fmtCtx);
        return;
    }

    AVPacket *pkt = av_packet_alloc();
    AVFrame *srcFrame = av_frame_alloc();
    bool decoded = false;

    // 画像は通常1パケット。確実に受け取るために Flush packet (nullptr) も考慮する
    bool eof = (av_read_frame(fmtCtx, pkt) < 0);
    while (!decoded) {
        if (avcodec_send_packet(decCtx, eof ? nullptr : pkt) == 0) {
            if (avcodec_receive_frame(decCtx, srcFrame) == 0) {
                decoded = true;
            }
        }
        if (!eof) {
            av_packet_unref(pkt);
            eof = true; // 1回読んだら次はFlush
        } else if (!decoded) {
            break; // Flushしても出なければ終了
        }
    }

    if (decoded) {
        // Convert to RGBA via sws_scale (same format as VideoDecoder, for compatibility with Vulkan/RHI backends)
        AVFrame *rgbaFrame = av_frame_alloc();
        rgbaFrame->format = AV_PIX_FMT_RGBA;
        rgbaFrame->width = srcFrame->width;
        rgbaFrame->height = srcFrame->height;
        if (av_frame_get_buffer(rgbaFrame, 0) < 0) {
            av_frame_free(&rgbaFrame);
            av_frame_free(&srcFrame);
            av_packet_free(&pkt);
            avcodec_free_context(&decCtx);
            avformat_close_input(&fmtCtx);
            return;
        }

        SwsContext *swsCtx = sws_getContext(srcFrame->width, srcFrame->height, static_cast<AVPixelFormat>(srcFrame->format), rgbaFrame->width, rgbaFrame->height, AV_PIX_FMT_RGBA, SWS_BILINEAR, nullptr, nullptr, nullptr);

        sws_scale(swsCtx, srcFrame->data, srcFrame->linesize, 0, srcFrame->height, rgbaFrame->data, rgbaFrame->linesize);
        sws_freeContext(swsCtx);

        // QQuickImageProvider 用に QImage としても保存する（これがプレビューされない直接的な原因）
        QImage img(rgbaFrame->data[0], rgbaFrame->width, rgbaFrame->height, rgbaFrame->linesize[0], QImage::Format_RGBA8888);
        m_cachedImage = img.copy();
        m_store->setFrameSafe(QString::number(clipId()), m_cachedImage);

        QVideoFrameFormat fmt(QSize(rgbaFrame->width, rgbaFrame->height), QVideoFrameFormat::Format_RGBA8888);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        auto *buf = new FFmpegVideoBuffer(rgbaFrame, fmt);
        QVideoFrame vf(buf, fmt);
#pragma clang diagnostic pop
        m_cachedVideoFrame = vf;

        // FFmpegVideoBuffer が av_frame_ref 済みなので解放可能
        av_frame_free(&rgbaFrame);

        m_store->setVideoFrameSafe(QString::number(clipId()), m_cachedVideoFrame);
        QMetaObject::invokeMethod(this, [this]() -> void { emit ready(); }, Qt::QueuedConnection);
    }

    av_frame_free(&srcFrame);
    av_packet_free(&pkt);
    avcodec_free_context(&decCtx);
    avformat_close_input(&fmtCtx);
}

} // namespace AviQtl::Core