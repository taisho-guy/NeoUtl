#include "video_encoder.hpp"
#include "settings_manager.hpp"
#include <QDebug>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/audio_fifo.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>
#include <libswscale/swscale.h>
}

namespace AviQtl::Core {

constexpr size_t MAX_QUEUE_SIZE = 16;

VideoEncoder::VideoEncoder(QObject *parent) : QObject(parent) {}

VideoEncoder::~VideoEncoder() { close(); }

void VideoEncoder::cleanup() {
    if (m_swsCtx != nullptr) {
        sws_freeContext(m_swsCtx);
        m_swsCtx = nullptr;
    }
    m_swsSrcFmt = -1;
    if (m_swrCtx != nullptr) {
        swr_free(&m_swrCtx);
        m_swrCtx = nullptr;
    }
    if (m_hwFrame != nullptr) {
        av_frame_free(&m_hwFrame);
    }
    if (m_swFrame != nullptr) {
        av_frame_free(&m_swFrame);
    }
    if (m_audioFrame != nullptr) {
        av_frame_free(&m_audioFrame);
    }
    if (m_audioFifo != nullptr) {
        av_audio_fifo_free(m_audioFifo);
        m_audioFifo = nullptr;
    }
    if (m_encCtx != nullptr) {
        avcodec_free_context(&m_encCtx);
    }
    if (m_fmtCtx != nullptr) {
        if ((m_fmtCtx->oformat->flags & AVFMT_NOFILE) == 0) {
            avio_closep(&m_fmtCtx->pb);
        }
        avformat_free_context(m_fmtCtx);
        m_fmtCtx = nullptr;
    }
    if (m_hwDeviceCtx != nullptr) {
        av_buffer_unref(&m_hwDeviceCtx);
    }
    if (m_audioEncCtx != nullptr) {
        avcodec_free_context(&m_audioEncCtx);
    }
}

auto VideoEncoder::initHardware(const QString &codecName) -> bool {
    int err = 0;
    AVHWDeviceType type = AV_HWDEVICE_TYPE_NONE;

    // コーデック名から適切なHWデバイスタイプを推論
    if (codecName.contains(QLatin1String("nvenc"))) {
        type = AV_HWDEVICE_TYPE_CUDA;
    } else if (codecName.contains(QLatin1String("vaapi"))) {
        type = AV_HWDEVICE_TYPE_VAAPI;
    } else if (codecName.contains(QLatin1String("qsv"))) {
        type = AV_HWDEVICE_TYPE_QSV;
    } else if (codecName.contains(QLatin1String("d3d11"))) {
        type = AV_HWDEVICE_TYPE_D3D11VA;
    } else if (codecName.contains(QLatin1String("dxva2"))) {
        type = AV_HWDEVICE_TYPE_DXVA2;
    } else if (codecName.contains(QLatin1String("videotoolbox"))) {
        type = AV_HWDEVICE_TYPE_VIDEOTOOLBOX;
    } else if (codecName.contains(QLatin1String("amf"))) {
        // AMFは通常DX11/Vulkanコンテキストを内部で作るが、FFmpeg上では明示的なデバイス作成が不要な場合が多い
        // 必要に応じて AV_HWDEVICE_TYPE_D3D11VA 等を割り当てる
        type = AV_HWDEVICE_TYPE_NONE;
    }

    if (type == AV_HWDEVICE_TYPE_NONE) {
        return true; // SWエンコードまたはデバイス不要
    }

    err = av_hwdevice_ctx_create(&m_hwDeviceCtx, type, nullptr, nullptr, 0);
    if (err < 0) {
        qWarning() << "Failed to create HW device context for" << codecName << "Error:" << err;
        return false;
    }
    qDebug() << "Hardware device initialized:" << av_hwdevice_get_type_name(type);
    return true;
}

auto VideoEncoder::open(const Config &config) -> bool {
    std::scoped_lock lock(m_mutex);
    cleanup();
    m_config = config;
    m_headerWritten = false;
    m_encodedFrameCount = 0;

    // 1. コンテナフォーマットの初期化
    avformat_alloc_output_context2(&m_fmtCtx, nullptr, nullptr, config.outputUrl.toStdString().c_str());
    if (m_fmtCtx == nullptr) {
        qWarning() << "Could not deduce output format from file extension.";
        return false;
    }

    // 2. コーデックの検索
    const AVCodec *codec = avcodec_find_encoder_by_name(config.codecName.toStdString().c_str());
    if (codec == nullptr) {
        qWarning() << "Codec not found:" << config.codecName;
        return false;
    }

    m_stream = avformat_new_stream(m_fmtCtx, codec);
    if (m_stream == nullptr) {
        return false;
    }

    m_encCtx = avcodec_alloc_context3(codec);
    if (m_encCtx == nullptr) {
        return false;
    }

    // 3. ハードウェア初期化
    if (!initHardware(config.codecName)) {
        return false;
    }

    if (m_hwDeviceCtx != nullptr) {
        // ハードウェアフレームコンテキストの設定
        AVBufferRef *hw_frames_ref = av_hwframe_ctx_alloc(m_hwDeviceCtx);
        auto *frames_ctx = reinterpret_cast<AVHWFramesContext *>(hw_frames_ref->data);

        // コーデックに応じたピクセルフォーマット設定
        if (config.codecName.contains(QLatin1String("vaapi"))) {
            frames_ctx->format = AV_PIX_FMT_VAAPI;
            frames_ctx->sw_format = AV_PIX_FMT_NV12;
        } else if (config.codecName.contains(QLatin1String("nvenc"))) {
            frames_ctx->format = AV_PIX_FMT_CUDA;
            frames_ctx->sw_format = AV_PIX_FMT_NV12; // or YUV420P
        } else if (config.codecName.contains(QLatin1String("qsv"))) {
            frames_ctx->format = AV_PIX_FMT_QSV;
            frames_ctx->sw_format = AV_PIX_FMT_NV12;
        }

        frames_ctx->width = config.width;
        frames_ctx->height = config.height;
        frames_ctx->initial_pool_size = SettingsManager::instance().value(QStringLiteral("hwFramePoolSize"), 32).toInt();

        if (av_hwframe_ctx_init(hw_frames_ref) >= 0) {
            m_encCtx->hw_frames_ctx = hw_frames_ref;
        }
    }

    // 4. エンコーダパラメータ設定
    m_encCtx->width = config.width;
    m_encCtx->height = config.height;
    m_encCtx->time_base = {.num = config.fps_den, .den = config.fps_num};
    m_stream->time_base = m_encCtx->time_base;
    m_stream->avg_frame_rate = {.num = config.fps_num, .den = config.fps_den};
    m_stream->r_frame_rate = m_stream->avg_frame_rate;

    // ピクセルフォーマットの自動選択
    if (m_encCtx->hw_frames_ctx != nullptr) {
        m_encCtx->pix_fmt = (reinterpret_cast<AVHWFramesContext *>(m_encCtx->hw_frames_ctx->data))->format;
    } else {
        // SWエンコードのデフォルト
        m_encCtx->pix_fmt = AV_PIX_FMT_YUV420P;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        if (codec->pix_fmts != nullptr) {
            m_encCtx->pix_fmt = codec->pix_fmts[0];
        }
#pragma clang diagnostic pop
    }

    // ビットレート制御 (CBR/VBR) - 簡易設定
    if (config.crf >= 0) {
        // CRF モード: libx264/libx265/libaom 系ソフトウェアエンコーダ向け
        av_opt_set_int(m_encCtx->priv_data, "crf", config.crf, 0);
        // HWエンコーダではフォールバックとして qp を設定
        av_opt_set_int(m_encCtx->priv_data, "qp", config.crf, 0);
    } else {
        m_encCtx->bit_rate = config.bitrate;
        m_encCtx->rc_max_rate = config.bitrate;
        m_encCtx->rc_buffer_size = static_cast<int>(config.bitrate / 2); // 0.5秒バッファ
    }

    // グローバルヘッダーが必要なコンテナ(mp4等)の場合
    if ((m_fmtCtx->oformat->flags & AVFMT_GLOBALHEADER) != 0) {
        m_encCtx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }

    if (avcodec_open2(m_encCtx, codec, nullptr) < 0) {
        qWarning() << "Could not open codec.";
        return false;
    }

    avcodec_parameters_from_context(m_stream->codecpar, m_encCtx);

    // 5. ファイルオープン
    if ((m_fmtCtx->oformat->flags & AVFMT_NOFILE) == 0) {
        if (avio_open(&m_fmtCtx->pb, config.outputUrl.toStdString().c_str(), AVIO_FLAG_WRITE) < 0) {
            qWarning() << "Could not open output file:" << config.outputUrl;
            return false;
        }
    }

    // 6. Frame allocation
    m_swFrame = av_frame_alloc();
    m_swFrame->format = AV_PIX_FMT_NV12; // SW intermediate buffer
    m_swFrame->width = config.width;
    m_swFrame->height = config.height;
    if (av_frame_get_buffer(m_swFrame, 32) < 0) {
        qWarning() << "Failed to allocate SW frame buffer.";
        cleanup();
        return false;
    }

    m_hwFrame = av_frame_alloc(); // For HW upload

    qDebug() << "VideoEncoder opened using codec:" << config.codecName;

    // エンコードスレッド開始
    m_stopEncoding = false;
    m_errorOccurred = false;
    {
        std::scoped_lock qlock(m_queueMutex);
        std::queue<EncodeTask> empty;
        std::swap(m_taskQueue, empty);
    }
    m_workerThread = std::thread(&VideoEncoder::encodingLoop, this);
    return true;
}

auto VideoEncoder::addAudioStream(int sampleRate, int channels) -> bool {
    std::scoped_lock lock(m_mutex);
    if (m_fmtCtx == nullptr) {
        return false;
    }

    const AVCodec *codec = avcodec_find_encoder_by_name(m_config.audioCodecName.toStdString().c_str());
    if (codec == nullptr) {
        codec = avcodec_find_encoder(AV_CODEC_ID_AAC); // フォールバック
    }

    if (codec == nullptr) {
        qWarning() << "AAC codec not found.";
        return false;
    }

    m_audioStream = avformat_new_stream(m_fmtCtx, codec);
    if (m_audioStream == nullptr) {
        return false;
    }

    m_audioEncCtx = avcodec_alloc_context3(codec);
    if (m_audioEncCtx == nullptr) {
        return false;
    }

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    m_audioEncCtx->sample_fmt = (codec->sample_fmts != nullptr) ? codec->sample_fmts[0] : AV_SAMPLE_FMT_FLTP;
#pragma clang diagnostic pop
    m_audioEncCtx->bit_rate = m_config.audioBitrate;
    m_audioEncCtx->sample_rate = sampleRate;
    av_channel_layout_default(&m_audioEncCtx->ch_layout, channels);
    m_audioStream->time_base = {.num = 1, .den = sampleRate};

    if ((m_fmtCtx->oformat->flags & AVFMT_GLOBALHEADER) != 0) {
        m_audioEncCtx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }

    if (avcodec_open2(m_audioEncCtx, codec, nullptr) < 0) {
        qWarning() << "Could not open audio codec.";
        return false;
    }

    avcodec_parameters_from_context(m_audioStream->codecpar, m_audioEncCtx);

    // FIFO and resampler initialization
    m_audioFifo = av_audio_fifo_alloc(m_audioEncCtx->sample_fmt, channels, 1024);
    m_audioFrame = av_frame_alloc();
    m_audioFrame->nb_samples = m_audioEncCtx->frame_size;
    m_audioFrame->format = m_audioEncCtx->sample_fmt;
    m_audioFrame->ch_layout = m_audioEncCtx->ch_layout;
    m_audioFrame->sample_rate = m_audioEncCtx->sample_rate;
    if (av_frame_get_buffer(m_audioFrame, 0) < 0) {
        qWarning() << "Failed to allocate audio frame buffer.";
        return false;
    }

    // Input (Float Interleaved) -> Output (Encoder Format, likely FLTP)
    swr_alloc_set_opts2(&m_swrCtx, &m_audioEncCtx->ch_layout, m_audioEncCtx->sample_fmt, m_audioEncCtx->sample_rate, &m_audioEncCtx->ch_layout, AV_SAMPLE_FMT_FLT, sampleRate, 0, nullptr);
    if (swr_init(m_swrCtx) < 0) {
        qWarning() << "Failed to initialize audio resampler.";
        return false;
    }

    qDebug() << "Audio stream added: AAC" << sampleRate << "Hz";
    return true;
}

auto VideoEncoder::writeHeaderIfNeeded() -> bool {
    if (m_headerWritten) {
        return true;
    }
    if (avformat_write_header(m_fmtCtx, nullptr) < 0) {
        qWarning() << "Error occurred when opening output file.";
        return false;
    }
    m_headerWritten = true;
    return true;
}

auto VideoEncoder::pushFrame(const QImage &img, int64_t pts) -> bool {
    if (m_errorOccurred) {
        return false;
    }

    std::unique_lock<std::mutex> lock(m_queueMutex);
    // バックプレッシャー: キューがいっぱいなら消費されるのを待つ
    m_queuePushCv.wait(lock, [this] -> bool { return m_taskQueue.size() < MAX_QUEUE_SIZE || m_stopEncoding; });

    if (m_stopEncoding) {
        return false;
    }

    EncodeTask task;
    task.type = EncodeTask::Video;
    task.videoImg = img;
    task.videoPts = pts;
    m_taskQueue.push(task);
    lock.unlock();
    m_queueCv.notify_one();
    return true;
}

auto VideoEncoder::processVideo(const QImage &img, int64_t pts) -> bool {
    // 入力QImageのフォーマットを直接FFmpegのAVPixelFormatにマッピングし、
    // 不要なconvertToFormatによるフレームコピーを回避する。
    // プラットフォームやグラフィックスAPIによって異なるピクセルレイアウトを正確に扱う。
    QImage sourceImg = img;
    AVPixelFormat srcPixFmt = AV_PIX_FMT_NONE;
    switch (img.format()) {
    case QImage::Format_RGBA8888:
    case QImage::Format_RGBX8888:
        srcPixFmt = AV_PIX_FMT_RGBA;
        break;
    case QImage::Format_ARGB32:
#if Q_BYTE_ORDER == Q_LITTLE_ENDIAN
        // QtのFormat_ARGB32はリトルエンド環境ではB-G-R-Aの順序(BGRA)
        srcPixFmt = AV_PIX_FMT_BGRA;
#else
        // ビッグエンド環境では正確なマッピングが複雑なためフォールバック
        sourceImg = img.convertToFormat(QImage::Format_RGBA8888);
        srcPixFmt = AV_PIX_FMT_RGBA;
#endif
        break;
    case QImage::Format_RGB888:
        srcPixFmt = AV_PIX_FMT_RGB24;
        break;
    default:
        // 未対応/プレマルチプライドフォーマットはRGBA8888に変換してフォールバック
        sourceImg = img.convertToFormat(QImage::Format_RGBA8888);
        srcPixFmt = AV_PIX_FMT_RGBA;
        break;
    }

    // 内部スレッドで実行される実際の映像エンコード処理
    std::scoped_lock lock(m_mutex);
    if (m_encCtx == nullptr) {
        return false;
    }

    if (!writeHeaderIfNeeded()) {
        return false;
    }

    // 1. QImage -> SW Frame (NV12) 変換
    if (m_swsCtx == nullptr || m_swsSrcFmt != static_cast<int>(srcPixFmt)) {
        if (m_swsCtx != nullptr) {
            sws_freeContext(m_swsCtx);
        }
        m_swsCtx = sws_getContext(sourceImg.width(), sourceImg.height(), srcPixFmt, m_config.width, m_config.height, AV_PIX_FMT_NV12, SWS_BILINEAR, nullptr, nullptr, nullptr);
        m_swsSrcFmt = static_cast<int>(srcPixFmt);
    }

    // SWフレームを書き込み可能にする
    if (av_frame_make_writable(m_swFrame) < 0) {
        return false;
    }

    // QImageのメモリレイアウトに合わせる
    const uint8_t *srcData[1] = {sourceImg.bits()};
    int srcLinesize[1] = {static_cast<int>(sourceImg.bytesPerLine())};

    // 変換実行
    sws_scale(m_swsCtx, srcData, srcLinesize, 0, sourceImg.height(), m_swFrame->data, m_swFrame->linesize);
    m_swFrame->pts = m_encodedFrameCount++;

    AVFrame *encodeFrame = m_swFrame;

    // 2. HWエンコードの場合: SW Frame -> HW Frame 転送 (CPU -> GPU Upload)
    if (m_encCtx->hw_frames_ctx != nullptr) {
        if (av_hwframe_get_buffer(m_encCtx->hw_frames_ctx, m_hwFrame, 0) < 0) {
            qWarning() << "Failed to allocate HW frame.";
            return false;
        }
        if (av_hwframe_transfer_data(m_hwFrame, m_swFrame, 0) < 0) {
            qWarning() << "Failed to transfer data to GPU.";
            av_frame_unref(m_hwFrame);
            return false;
        }
        m_hwFrame->pts = m_swFrame->pts;
        encodeFrame = m_hwFrame;
    }

    // 3. エンコード
    int ret = avcodec_send_frame(m_encCtx, encodeFrame);

    // EAGAINハンドリング: 入力バッファがいっぱいの場合、出力を読み出して空ける
    while (ret == AVERROR(EAGAIN)) {
        bool packetRead = false;
        while (true) {
            AVPacket *pkt = av_packet_alloc();
            int rxRet = avcodec_receive_packet(m_encCtx, pkt);
            if (rxRet == AVERROR(EAGAIN) || rxRet == AVERROR_EOF) {
                av_packet_free(&pkt);
                break;
            }
            if (rxRet < 0) {
                av_packet_free(&pkt);
                return false;
            }

            if (pkt->duration == 0) {
                pkt->duration = 1;
            }
            av_packet_rescale_ts(pkt, m_encCtx->time_base, m_stream->time_base);
            pkt->stream_index = m_stream->index;
            av_interleaved_write_frame(m_fmtCtx, pkt);
            av_packet_free(&pkt);
            packetRead = true;
        }
        if (!packetRead) {
            break; // 進展がない場合は抜ける
        }
        ret = avcodec_send_frame(m_encCtx, encodeFrame);
    }

    if (encodeFrame == m_hwFrame) {
        av_frame_unref(m_hwFrame);
    }

    if (ret < 0) {
        qWarning() << "Error sending frame to codec:" << ret;
        return false;
    }

    while (ret >= 0) {
        AVPacket *pkt = av_packet_alloc();
        ret = avcodec_receive_packet(m_encCtx, pkt);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            av_packet_free(&pkt);
            break;
        }
        if (ret < 0) {
            qWarning() << "Error during encoding.";
            av_packet_free(&pkt);
            return false;
        }

        // durationが設定されていない場合のフォールバック (固定フレームレート)
        if (pkt->duration == 0) {
            pkt->duration = 1;
        }

        av_packet_rescale_ts(pkt, m_encCtx->time_base, m_stream->time_base);
        pkt->stream_index = m_stream->index;

        av_interleaved_write_frame(m_fmtCtx, pkt);
        av_packet_free(&pkt); // パケット構造体とデータの解放
    }

    return true;
}

auto VideoEncoder::pushAudio(const float *samples, int sampleCount) -> bool {
    if (m_errorOccurred) {
        return false;
    }

    std::unique_lock<std::mutex> lock(m_queueMutex);
    m_queuePushCv.wait(lock, [this] -> bool { return m_taskQueue.size() < MAX_QUEUE_SIZE || m_stopEncoding; });

    if (m_stopEncoding) {
        return false;
    }

    EncodeTask task;
    task.type = EncodeTask::Audio;
    task.audioSamples.assign(samples, samples + sampleCount);
    m_taskQueue.push(task);
    lock.unlock();
    m_queueCv.notify_one();
    return true;
}

auto VideoEncoder::processAudio(const std::vector<float> &samples) -> bool {
    // 内部スレッドで実行される実際の音声エンコード処理
    std::scoped_lock lock(m_mutex);
    if ((m_audioEncCtx == nullptr) || (m_audioFifo == nullptr)) {
        return false;
    }

    // 不正な浮動小数点数 (NaN/Inf) がエンコーダに渡されるのを防ぐためのサニタイズ処理
    std::vector<float> cleanSamples = samples;
    for (float &sample : cleanSamples) {
        if (std::isnan(sample) || std::isinf(sample)) {
            sample = 0.0F;
        }
    }

    // 1. リサンプリング & フォーマット変換 (Float -> FLTP等)
    // 一時バッファに変換
    uint8_t **convertedData = nullptr;
    int linesize = 0;
    // 入力はインターリーブされたステレオFloatなので、サンプル数は全要素数をチャンネル数で割った値
    const int sampleCount = static_cast<int>(cleanSamples.size() / m_audioEncCtx->ch_layout.nb_channels);
    if (sampleCount <= 0) {
        return true;
    }
    av_samples_alloc_array_and_samples(&convertedData, &linesize, m_audioEncCtx->ch_layout.nb_channels, sampleCount, m_audioEncCtx->sample_fmt, 0);

    const uint8_t *inputData[1] = {reinterpret_cast<const uint8_t *>(cleanSamples.data())};
    swr_convert(m_swrCtx, convertedData, sampleCount, inputData, sampleCount);

    // 2. FIFOに追加
    av_audio_fifo_write(m_audioFifo, reinterpret_cast<void **>(convertedData), sampleCount);

    if (convertedData != nullptr) {
        av_freep(static_cast<void *>(&convertedData[0])); // sample buffer
        av_freep(static_cast<void *>(&convertedData));    // pointer array (av_malloc'd, not C free)
    }

    // 3. エンコーダーのフレームサイズ分溜まったらエンコード
    while (av_audio_fifo_size(m_audioFifo) >= m_audioEncCtx->frame_size) {
        if (av_frame_make_writable(m_audioFrame) < 0) {
            break;
        }

        // FIFOから読み出し
        av_audio_fifo_read(m_audioFifo, reinterpret_cast<void **>(m_audioFrame->data), m_audioEncCtx->frame_size);

        m_audioFrame->pts = m_audioPts;
        m_audioPts += m_audioFrame->nb_samples;

        // エンコード
        int ret = avcodec_send_frame(m_audioEncCtx, m_audioFrame);

        // EAGAINハンドリング
        while (ret == AVERROR(EAGAIN)) {
            bool packetRead = false;
            while (true) {
                AVPacket *pkt = av_packet_alloc();
                int rxRet = avcodec_receive_packet(m_audioEncCtx, pkt);
                if (rxRet == AVERROR(EAGAIN) || rxRet == AVERROR_EOF) {
                    av_packet_free(&pkt);
                    break;
                }
                if (rxRet < 0) {
                    av_packet_free(&pkt);
                    return false;
                }

                av_packet_rescale_ts(pkt, m_audioEncCtx->time_base, m_audioStream->time_base);
                pkt->stream_index = m_audioStream->index;
                av_interleaved_write_frame(m_fmtCtx, pkt);
                av_packet_free(&pkt);
                packetRead = true;
            }
            if (!packetRead) {
                break;
            }
            ret = avcodec_send_frame(m_audioEncCtx, m_audioFrame);
        }

        if (ret < 0) {
            qWarning() << "Error sending audio frame to codec:" << ret;
            return false;
        }

        while (ret >= 0) {
            AVPacket *pkt = av_packet_alloc();
            ret = avcodec_receive_packet(m_audioEncCtx, pkt);
            if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
                av_packet_free(&pkt);
                break;
            }
            if (ret < 0) {
                av_packet_free(&pkt);
                return false;
            }

            av_packet_rescale_ts(pkt, m_audioEncCtx->time_base, m_audioStream->time_base);
            pkt->stream_index = m_audioStream->index;
            av_interleaved_write_frame(m_fmtCtx, pkt);
            av_packet_free(&pkt);
        }
    }
    return true;
}

void VideoEncoder::encodingLoop() {
    while (true) {
        EncodeTask task;
        {
            std::unique_lock<std::mutex> lock(m_queueMutex);
            m_queueCv.wait(lock, [this] -> bool { return !m_taskQueue.empty() || m_stopEncoding; });

            if (m_taskQueue.empty() && m_stopEncoding) {
                break; // 終了
            }

            task = m_taskQueue.front();
            m_taskQueue.pop();
            // キューに空きができたことを通知
            m_queuePushCv.notify_one();
        }

        bool success = true;
        if (task.type == EncodeTask::Video) {
            success = processVideo(task.videoImg, task.videoPts);
        } else if (task.type == EncodeTask::Audio) {
            success = processAudio(task.audioSamples);
        }

        if (!success) {
            m_errorOccurred = true;
            qWarning() << "Encoding task failed in worker thread.";
        }
    }
}

void VideoEncoder::close() {
    // 1. スレッドに終了シグナルを送る
    {
        std::scoped_lock lock(m_queueMutex);
        m_stopEncoding = true;
        m_queueCv.notify_all();
        m_queuePushCv.notify_all(); // push待ちも解除
    }

    // 2. スレッド終了待ち（残りのキュー処理完了を待つ）
    if (m_workerThread.joinable()) {
        m_workerThread.join();
    }

    std::scoped_lock lock(m_mutex);
    if (m_encCtx == nullptr) {
        return;
    }

    writeHeaderIfNeeded(); // 何も書き込まれずにcloseされた場合の安全策

    // フラッシュ処理
    int ret = avcodec_send_frame(m_encCtx, nullptr);
    while (ret >= 0) {
        AVPacket *pkt = av_packet_alloc();
        ret = avcodec_receive_packet(m_encCtx, pkt);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            av_packet_free(&pkt);
            break;
        }
        if (ret < 0) {
            av_packet_free(&pkt);
            qWarning() << "Error flushing video encoder:" << ret;
            break;
        }
        av_packet_rescale_ts(pkt, m_encCtx->time_base, m_stream->time_base);
        pkt->stream_index = m_stream->index;
        av_interleaved_write_frame(m_fmtCtx, pkt);
        av_packet_free(&pkt);
    }

    // 音声フラッシュ
    if (m_audioEncCtx != nullptr) {
        ret = avcodec_send_frame(m_audioEncCtx, nullptr);
        while (ret >= 0) {
            AVPacket *pkt = av_packet_alloc();
            ret = avcodec_receive_packet(m_audioEncCtx, pkt);
            if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
                av_packet_free(&pkt);
                break;
            }
            if (ret < 0) {
                av_packet_free(&pkt);
                qWarning() << "Error flushing audio encoder:" << ret;
                break;
            }
            av_packet_rescale_ts(pkt, m_audioEncCtx->time_base, m_audioStream->time_base);
            pkt->stream_index = m_audioStream->index;
            av_interleaved_write_frame(m_fmtCtx, pkt);
            av_packet_free(&pkt);
        }
    }

    av_write_trailer(m_fmtCtx);
    cleanup();
    qDebug() << "VideoEncoder closed.";
}

} // namespace AviQtl::Core