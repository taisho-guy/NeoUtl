#include "timeline_media_manager.hpp"
#include "audio_decoder.hpp"
#include "effect_registry.hpp"
#include "engine/audio_mixer.hpp"
#include "image_decoder.hpp"
#include "media_decoder.hpp"
#include "timeline_controller.hpp"
#include "video_decoder.hpp"
#include "video_frame_store.hpp"
#include <algorithm>
#include <cmath>

namespace AviQtl::UI {

TimelineMediaManager::TimelineMediaManager(TimelineController *controller, QObject *parent) : QObject(parent), m_controller(controller), m_audioMixer(new AviQtl::Engine::AudioMixer(this)) {}

void TimelineMediaManager::setVideoFrameStore(AviQtl::Core::VideoFrameStore *store) {
    m_videoFrameStore = store;
    updateMediaDecoders();
}

void TimelineMediaManager::onPlayingChanged() {
    bool playing = m_controller->transport()->isPlaying();
    for (const auto &decoder : std::as_const(m_decoders)) {
        if (!decoder) {
            continue;
        }
        decoder->setPlaying(playing);
    }
}

void TimelineMediaManager::onCurrentFrameChanged() {

    int nextFrame = m_controller->transport()->currentFrame();
    double fps = m_controller->project()->fps();
    if (m_controller->transport()->isPlaying()) {
        int sampleRate = m_controller->project()->sampleRate();
        m_audioMixer->processFrame(nextFrame, fps, static_cast<int>(std::round(static_cast<double>(sampleRate) / fps)));
    }

    for (auto it = m_decoders.begin(); it != m_decoders.end(); ++it) {
        const auto *clip = m_controller->timeline()->findClipById(it.key());
        if ((clip == nullptr) || nextFrame < clip->startFrame || nextFrame >= clip->startFrame + clip->durationFrames) {
            continue;
        }

        if (auto *vid = qobject_cast<AviQtl::Core::VideoDecoder *>(it.value())) {
            updateVideoClipFrame(vid, clip, nextFrame - clip->startFrame);
        }

        if (auto *img = qobject_cast<AviQtl::Core::ImageDecoder *>(it.value())) {
            img->seek(0); // 描画を強制
        }

        if (auto *aud = qobject_cast<AviQtl::Core::AudioDecoder *>(it.value())) {
            const int relFrame = nextFrame - clip->startFrame;
            const double relTime = static_cast<double>(relFrame) / fps;
            double audioTime = 0.0;

            for (const auto *eff : clip->effects) {
                if (eff->id() != QStringLiteral("audio")) {
                    continue;
                }

                const QString playMode = eff->params().value(QStringLiteral("playMode"), "開始時間＋再生速度").toString();

                if (playMode == QStringLiteral("時間直接指定")) {
                    audioTime = eff->evaluatedParam(QStringLiteral("directTime"), relFrame, fps).toDouble();
                } else {
                    const double startTime = eff->params().value(QStringLiteral("startTime"), 0.0).toDouble();
                    const double speed = eff->params().value(QStringLiteral("speed"), 100.0).toDouble();
                    audioTime = (relTime * (speed / 100.0)) + startTime;
                }
                break;
            }
            aud->seek(static_cast<qint64>(audioTime * 1000.0));
        }
    }
}

void TimelineMediaManager::syncPlaybackSpeed() {
    double speed = m_controller->transport()->playbackSpeed();
    for (const auto &decoder : std::as_const(m_decoders)) {
        if (!decoder) {
            continue;
        }
        decoder->setPlaybackRate(speed);
    }
    if (m_audioMixer) {
        m_audioMixer->setPlaybackSpeed(speed);
    }
}

void TimelineMediaManager::updateAudioSampleRate() {
    int rate = m_controller->project()->sampleRate();
    if (m_audioMixer) {
        m_audioMixer->setSampleRate(rate);
    }
    for (const auto &decoder : std::as_const(m_decoders)) {
        if (!decoder) {
            continue;
        }
        decoder->setSampleRate(rate);
    }
}

auto TimelineMediaManager::getClipSourceUrl(const ClipData &clip) -> QUrl {
    const EffectModel *effModel = nullptr;
    for (const auto *eff : std::as_const(clip.effects)) {
        if (eff->id() == clip.type) {
            effModel = eff;
            break;
        }
    }
    if (effModel == nullptr) {
        return {};
    }
    // 音声以外は通常 "path" パラメータにファイルパスが入っている
    QString path = effModel->params().value(clip.type == QStringLiteral("audio") ? QLatin1String("source") : QLatin1String("path")).toString();
    return QUrl::fromLocalFile(path);
}

void TimelineMediaManager::updateMediaDecoders() {
    // 巨大な QList<ClipData> のコピー作成を避け、元のデータ構造を直接走査する
    const auto &scenes = m_controller->timeline()->getAllScenes();
    QSet<int> currentClipIds;
    QHash<int, int> clipToScene;

    for (const auto &scene : std::as_const(scenes)) {
        for (const auto &clip : std::as_const(scene.clips)) {
            if (clip.type != "video" && clip.type != "audio" && clip.type != QStringLiteral("image")) {
                continue;
            }

            currentClipIds.insert(clip.id);
            clipToScene.insert(clip.id, scene.id);

            QUrl sourceUrl = getClipSourceUrl(clip);
            if (!sourceUrl.isValid() || sourceUrl.isEmpty()) {
                auto it = m_decoders.find(clip.id);
                if (it != m_decoders.end()) {
                    auto decoder = it.value();
                    if (qobject_cast<AviQtl::Core::AudioDecoder *>(decoder) != nullptr) {
                        m_audioMixer->unregisterDecoder(clip.id);
                    }
                    if (decoder) {
                        decoder->deleteLater();
                    }
                    m_decoders.erase(it);
                }
                continue;
            }

            auto itExisting = m_decoders.find(clip.id);
            if (itExisting != m_decoders.end()) {
                AviQtl::Core::MediaDecoder *existingDecoder = itExisting.value();
                if (existingDecoder != nullptr) {
                    // If the source has changed, we must recreate the decoder
                    if (existingDecoder->source() != sourceUrl) {
                        if (qobject_cast<AviQtl::Core::AudioDecoder *>(existingDecoder) != nullptr) {
                            m_audioMixer->unregisterDecoder(clip.id);
                        }
                        existingDecoder->deleteLater();
                        m_decoders.erase(itExisting);
                    } else {
                        continue;
                    }
                } else {
                    m_decoders.erase(itExisting);
                }
            }

            AviQtl::Core::MediaDecoder *decoder = nullptr;
            if (clip.type == QStringLiteral("video")) {
                if (m_videoFrameStore == nullptr) {
                    continue;
                }
                decoder = new AviQtl::Core::VideoDecoder(clip.id, sourceUrl, m_videoFrameStore, this);
            } else if (clip.type == QStringLiteral("image")) {
                if (m_videoFrameStore == nullptr) {
                    continue;
                }
                decoder = new AviQtl::Core::ImageDecoder(clip.id, sourceUrl, m_videoFrameStore, this);
            } else if (clip.type == QStringLiteral("audio")) {
                decoder = new AviQtl::Core::AudioDecoder(clip.id, sourceUrl, this);
                if (auto *audioDecoder = qobject_cast<AviQtl::Core::AudioDecoder *>(decoder)) {
                    m_audioMixer->registerDecoder(clip.id, audioDecoder);
                }
            }

            if (decoder != nullptr) {
                m_decoders.insert(clip.id, decoder);
                int cid = clip.id;
                // 画像や動画のデコード準備ができたらUIへ通知する
                connect(decoder, &AviQtl::Core::MediaDecoder::ready, this, [this, cid]() -> void { emit frameUpdated(cid); });

                if (auto *vid = qobject_cast<AviQtl::Core::VideoDecoder *>(decoder)) {
                    connect(decoder, &AviQtl::Core::MediaDecoder::frameReady, this, [this, cid](int) -> void { emit frameUpdated(cid); });
                    connect(vid, &AviQtl::Core::VideoDecoder::videoMetaReady, this, [this, cid](int totalFrameCount, double sourceFps) -> void {
                        const auto *clip = m_controller->timeline()->findClipById(cid);
                        if (!clip || clip->type != QStringLiteral("video")) {
                            return;
                        }

                        int startVideoFrame = 0;
                        double speed = 100.0;
                        for (const auto *eff : clip->effects) {
                            if (eff->id() != QStringLiteral("video")) {
                                continue;
                            }
                            const QString playMode = eff->params().value(QStringLiteral("playMode"), "開始フレーム＋再生速度").toString();
                            if (playMode == QStringLiteral("フレーム直接指定")) {
                                return;
                            }
                            startVideoFrame = eff->params().value(QStringLiteral("startFrame"), 0).toInt();
                            speed = eff->params().value(QStringLiteral("speed"), 100.0).toDouble();
                            break;
                        }

                        if (speed <= 0.0 || sourceFps <= 0.0) {
                            return;
                        }

                        const int projectFps = static_cast<int>(m_controller->project()->fps());
                        const double startSec = static_cast<double>(startVideoFrame) / sourceFps;
                        const double remainingSec = (static_cast<double>(totalFrameCount) / sourceFps) - startSec;
                        if (remainingSec <= 0.0) {
                            return;
                        }

                        const int maxDuration = static_cast<int>(remainingSec / (speed / 100.0) * projectFps);
                        if (maxDuration > 0 && clip->durationFrames > maxDuration) {
                            m_controller->updateClip(clip->id, clip->layer, clip->startFrame, maxDuration);
                        }
                    });
                }
                decoder->scheduleStart(); // 非同期起動
            }
        }
    }

    for (auto it = m_decoders.begin(); it != m_decoders.end();) {
        if (!currentClipIds.contains(it.key())) {
            if (qobject_cast<AviQtl::Core::AudioDecoder *>(it.value()) != nullptr) {
                m_audioMixer->unregisterDecoder(it.key());
            }
            if (m_videoFrameStore != nullptr) {
                // キー形式を ImageDecoder 等と統一 (clipId のみを使用)
                m_videoFrameStore->invalidateFrame(QString::number(it.key()));
            }
            if (it.value()) {
                it.value()->deleteLater();
            }
            it = m_decoders.erase(it);
        } else {
            ++it;
        }
    }
}

void TimelineMediaManager::updateVideoClipFrame(AviQtl::Core::VideoDecoder *vid, const ClipData *clip, int relFrame) {
    if ((vid == nullptr) || (clip == nullptr) || (m_controller == nullptr) || (m_controller->project() == nullptr)) {
        return;
    }

    relFrame = std::max(relFrame, 0);
    const double fps = [&]() -> double {
        const double f = m_controller->project()->fps();
        return f > 0.0 ? f : 30.0;
    }();
    const double relTime = static_cast<double>(relFrame) / fps;

    for (const auto *eff : clip->effects) {
        if ((eff == nullptr) || eff->id() != QStringLiteral("video")) {
            continue;
        }

        const QString playMode = eff->params().value(QStringLiteral("playMode"), "開始フレーム＋再生速度").toString();

        if (playMode == QStringLiteral("フレーム直接指定")) {
            const int absFrame = eff->evaluatedParam(QStringLiteral("directFrame"), relFrame, fps).toInt();
            vid->seekToFrame(absFrame, vid->sourceFps());
        } else {
            const int startFrame = eff->evaluatedParam(QStringLiteral("startFrame"), 0, fps).toInt();
            const double speed = eff->evaluatedParam(QStringLiteral("speed"), 100, fps).toDouble();

            double vfps = vid->sourceFps();
            if (vfps <= 0.0) {
                vfps = fps;
            }

            const double startSec = static_cast<double>(startFrame) / vfps;
            const double targetSec = startSec + (relTime * (speed / 100.0));
            vid->seekToTime(targetSec);
        }
        return;
    }
}

auto TimelineMediaManager::sceneIdForClip(int clipId) const -> int {
    for (const auto &scene : m_controller->timeline()->getAllScenes()) {
        for (const auto &clip : std::as_const(scene.clips)) {
            if (clip.id == clipId) {
                return scene.id;
            }
        }
    }
    return -1;
}

void TimelineMediaManager::requestVideoFrame(int clipId, int relFrame) { // NOLINT(bugprone-easily-swappable-parameters)
    if ((m_controller == nullptr) || (m_controller->timeline() == nullptr)) {
        return;
    }

    const ClipData *targetClip = m_controller->timeline()->findClipById(clipId);
    if (targetClip == nullptr) {
        return;
    }

    auto *vid = qobject_cast<AviQtl::Core::VideoDecoder *>(decoderForClip(clipId));
    if (vid == nullptr) {
        return;
    }

    updateVideoClipFrame(vid, targetClip, relFrame);
}

void TimelineMediaManager::requestImageLoad(int clipId, const QString &path) {
    if ((m_videoFrameStore == nullptr) || path.isEmpty() || clipId <= 0) {
        return;
    }

    const QUrl url = QUrl::fromLocalFile(path);

    if (auto it = m_imageDecoders.find(clipId); it != m_imageDecoders.end()) {
        auto existing = it.value();
        if (existing && existing->source() == url) {
            return;
        }
    }

    auto *decoder = new AviQtl::Core::ImageDecoder(clipId, url, m_videoFrameStore, this);
    connect(decoder, &AviQtl::Core::MediaDecoder::ready, this, [this, clipId]() -> void { emit frameUpdated(clipId); });
    m_imageDecoders.insert(clipId, decoder);
    decoder->load();
}

} // namespace AviQtl::UI