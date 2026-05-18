#include "core/include/interpolation_engine.hpp"
#include "ecs.hpp"
#include "ecs_profiler.hpp"
#include "engine/plugin/audio_plugin_manager.hpp"
#include "ui/include/bridge/core_bridge.hpp"
#include <QDebug>
#include <cassert>
#include <cmath>

// ─────────────────────────────────────────────────────────────────────────────
// Phase 2.4: CommandSystem
//
// CoreBridge の SPSC リングバッファからコマンドを取り出し、ECS 状態に反映する。
// CommandSystem は ECS の毎フレーム先頭で実行される (仕様書第5章 System 実行順序)。
//
// 実行順序:
//   CommandSystem → InterpolationSystem (Phase 4) → TransformSystem (Phase 4)
//   → RenderSystem (Phase 5)
// ─────────────────────────────────────────────────────────────────────────────

namespace AviQtl::Engine::Timeline {

// ─── CommandSystem ────────────────────────────────────────────────────────────

void ECS::runCommandSystem(AviQtl::UI::CoreBridge &bridge) {
    AviQtl::UI::CoreBridge::Command cmd;
    while (bridge.dequeueCommand(cmd)) {
        switch (cmd.type) {
        case AviQtl::UI::CoreBridge::CommandType::Seek:
            m_currentFrame = cmd.value;
            bridge.notifyFrameAdvanced(m_currentFrame);
            break;
        case AviQtl::UI::CoreBridge::CommandType::Play:
            m_isPlaying = true;
            break;
        case AviQtl::UI::CoreBridge::CommandType::Pause:
            m_isPlaying = false;
            break;
        }
    }
}

// ─── ECS コンストラクタ / インスタンス ────────────────────────────────────────

ECS::ECS() : m_editIndex(1) {
    m_activeIndex.store(0, std::memory_order_relaxed);
    for (auto &f : m_dirtyFlags)
        f.fullSync = true;
}

auto ECS::instance() -> ECS & {
    static ECS inst;
    return inst;
}

// ─── syncClipIds ──────────────────────────────────────────────────────────────

void ECS::syncClipIds(const std::bitset<MAX_CLIP_ID> &aliveFlags) {
    auto &editState = m_buffers[m_editIndex];
    bool changed = false;
    changed |= editState.transforms.syncAlive(aliveFlags);
    changed |= editState.renderStates.syncAlive(aliveFlags);
    changed |= editState.audioStates.syncAlive(aliveFlags);
    changed |= editState.metadataStates.syncAlive(aliveFlags);

    // Phase 4 同期
    changed |= editState.keyframeRefs.syncAlive(aliveFlags);
    changed |= editState.ecsTransforms.syncAlive(aliveFlags);
    changed |= editState.globalMatrices.syncAlive(aliveFlags);

    if (changed) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
}

// ─── updateClipState ──────────────────────────────────────────────────────────

void ECS::updateClipState(int clipId, int layer, double time, int startFrame, int durationFrames) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    if (!editState.transforms.contains(clipId)) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
    auto &transform = editState.transforms[clipId];
    bool changed = (transform.layer != layer) || (std::abs(transform.timePosition - time) > 0.001) || (transform.startFrame != startFrame) || (transform.durationFrames != durationFrames);
    if (changed) {
        transform.layer = layer;
        transform.timePosition = time;
        transform.startFrame = startFrame;
        transform.durationFrames = durationFrames;
        auto &render = editState.renderStates[clipId];
        render.needsUpdate = true;
        editState.renderGraphDirty = true;
    }

    for (int i = 1; i <= 2; ++i) {
        auto &df = m_dirtyFlags[(m_editIndex + i) % 3];
        if (!df.dirty.test(static_cast<std::size_t>(clipId))) {
            df.dirty.set(static_cast<std::size_t>(clipId));
            df.dirtyIds.push_back(clipId);
        }
    }
    ECS_PROF_INC(dirtyBitSetCount);
}

// ─── updateAudioClipState ─────────────────────────────────────────────────────

void ECS::updateAudioClipState(int clipId, int startFrame, int durationFrames, float volume, float pan, bool mute) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    if (!editState.audioStates.contains(clipId)) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
    auto &audio = editState.audioStates[clipId];
    audio.clipId = clipId;
    audio.startFrame = startFrame;
    audio.durationFrames = durationFrames;
    audio.volume = volume;
    audio.pan = pan;
    audio.mute = mute;

    for (int i = 1; i <= 2; ++i) {
        auto &df = m_dirtyFlags[(m_editIndex + i) % 3];
        if (!df.dirty.test(static_cast<std::size_t>(clipId))) {
            df.dirty.set(static_cast<std::size_t>(clipId));
            df.dirtyIds.push_back(clipId);
        }
    }
    ECS_PROF_INC(dirtyBitSetCount);
}

// ─── updateMetadata ───────────────────────────────────────────────────────────

static auto parseColorRGBA(const QString &colorStr) -> uint32_t {
    QString s = colorStr.trimmed();
    if (s.startsWith(u'#'))
        s.remove(0, 1);
    bool ok = false;
    const uint32_t val = s.toUInt(&ok, 16);
    if (!ok)
        return 0xFF000000u;
    if (s.length() == 6)
        return 0xFF000000u | val;
    return val;
}

void ECS::updateMetadata(int clipId, const QString &name, const QString &source, const QString &type, const QString &color) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    if (!editState.metadataStates.contains(clipId)) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
    const uint32_t nId = m_stringTable.intern(name.toStdString());
    const uint32_t sId = m_stringTable.intern(source.toStdString());
    const uint32_t tId = m_stringTable.intern(type.toStdString());
    const uint32_t cRGBA = parseColorRGBA(color);
    auto &meta = editState.metadataStates[clipId];
    if (meta.clipId != static_cast<int32_t>(clipId) || meta.nameId != nId || meta.sourceId != sId || meta.typeId != tId || meta.colorRGBA != cRGBA) {
        meta.clipId = static_cast<int32_t>(clipId);
        meta.nameId = nId;
        meta.sourceId = sId;
        meta.typeId = tId;
        meta.colorRGBA = cRGBA;
    }

    for (int i = 1; i <= 2; ++i) {
        auto &df = m_dirtyFlags[(m_editIndex + i) % 3];
        if (!df.dirty.test(static_cast<std::size_t>(clipId))) {
            df.dirty.set(static_cast<std::size_t>(clipId));
            df.dirtyIds.push_back(clipId);
        }
    }
}

// ─── commit ───────────────────────────────────────────────────────────────────

void ECS::commit() {
    ECS_PROF_INC(commitCount);

    const int justWritten = m_editIndex;
    const int active = m_activeIndex.load(std::memory_order_acquire);
    const int pending = m_pendingIndex.load(std::memory_order_acquire);

    // 次に書き込むバッファを選択 (justWritten, active, pending を避ける)
    int next = -1;
    for (int c = 0; c < 3; ++c) {
        if (c == justWritten || c == active || (pending != -1 && c == pending))
            continue;
        next = c;
        break;
    }
    // 全て埋まっている場合（理論上稀）は、pending を上書きする
    if (next == -1)
        next = (pending != -1) ? pending : (justWritten + 1) % 3;

    m_editIndex = next;

    auto &df = m_dirtyFlags[m_editIndex];
    if (df.fullSync) {
        m_buffers[m_editIndex] = m_buffers[justWritten];
        df.fullSync = false;
        df.dirty.reset();
        df.dirtyIds.clear();
    } else {
        const auto &src = m_buffers[justWritten];
        auto &dst = m_buffers[m_editIndex];
        dst.renderGraphDirty = src.renderGraphDirty;

        // dirtyIds を使ってピンポイントで同期
        for (int id : df.dirtyIds) {
            if (const auto *s = src.transforms.find(id))
                dst.transforms[id] = *s;
            if (const auto *s = src.renderStates.find(id))
                dst.renderStates[id] = *s;
            if (const auto *s = src.audioStates.find(id))
                dst.audioStates[id] = *s;
            if (const auto *s = src.metadataStates.find(id))
                dst.metadataStates[id] = *s;

            // Phase 4 同期
            if (const auto *s = src.keyframeRefs.find(id))
                dst.keyframeRefs[id] = *s;
            if (const auto *s = src.ecsTransforms.find(id))
                dst.ecsTransforms[id] = *s;
            if (const auto *s = src.globalMatrices.find(id))
                dst.globalMatrices[id] = *s;
        }
        df.dirty.reset();
        df.dirtyIds.clear();
    }

    m_pendingIndex.store(justWritten, std::memory_order_release);
}

// ─── getSnapshot ─────────────────────────────────────────────────────────────

auto ECS::getSnapshot() const -> const ECSState * {
    int pending = m_pendingIndex.load(std::memory_order_acquire);
    if (pending != -1) {
        if (m_pendingIndex.compare_exchange_strong(pending, -1, std::memory_order_acq_rel, std::memory_order_relaxed)) {
            m_activeIndex.store(pending, std::memory_order_release);
        }
    }
    return &m_buffers[m_activeIndex.load(std::memory_order_acquire)];
}

// ─── writeSSBOLayout ─────────────────────────────────────────────────────────

void ECS::writeSSBOLayout(GpuClipSoA &out) const {
    ECS_PROF_INC(ssboWriteCount);
    const ECSState *state = getSnapshot();
    out.count = 0;
    state->transforms.forEach([&](int clipId, const TransformComponent &tc) {
        if (out.count >= MAX_ACTIVE_CLIPS)
            return;
        const int idx = out.count++;
        out.clipIds[idx] = static_cast<int32_t>(clipId);
        out.layers[idx] = static_cast<int32_t>(tc.layer);
        out.timePositions[idx] = static_cast<float>(tc.timePosition);
        out.startFrames[idx] = static_cast<int32_t>(tc.startFrame);
        out.durationFrames[idx] = static_cast<int32_t>(tc.durationFrames);
        if (const auto *ac = state->audioStates.find(clipId)) {
            out.volumes[idx] = ac->volume;
            out.pans[idx] = ac->pan;
            out.mutes[idx] = ac->mute ? 1 : 0;
        } else {
            out.volumes[idx] = 1.0f;
            out.pans[idx] = 0.0f;
            out.mutes[idx] = 0;
        }
    });
}

// ─── Phase 4 サポート API & Systems ──────────────────────────────────────────

void ECS::updateKeyframeRef(int clipId, uint32_t effectId) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    if (!editState.keyframeRefs.contains(clipId)) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
    auto &kr = editState.keyframeRefs[clipId];
    kr.clipId = static_cast<uint32_t>(clipId);
    kr.effectId = effectId;

    for (int i = 1; i <= 2; ++i) {
        auto &df = m_dirtyFlags[(m_editIndex + i) % 3];
        if (!df.dirty.test(static_cast<std::size_t>(clipId))) {
            df.dirty.set(static_cast<std::size_t>(clipId));
            df.dirtyIds.push_back(clipId);
        }
    }
}

void ECS::updateEcsTransform(int clipId, float x, float y, float z, float scaleX, float scaleY, float rotX, float rotY, float rotZ, float opacity) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    if (!editState.ecsTransforms.contains(clipId)) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
    auto &et = editState.ecsTransforms[clipId];
    et.x = x;
    et.y = y;
    et.z = z;
    et.scaleX = scaleX;
    et.scaleY = scaleY;
    et.rotX = rotX;
    et.rotY = rotY;
    et.rotZ = rotZ;
    et.opacity = opacity;

    for (int i = 1; i <= 2; ++i) {
        auto &df = m_dirtyFlags[(m_editIndex + i) % 3];
        if (!df.dirty.test(static_cast<std::size_t>(clipId))) {
            df.dirty.set(static_cast<std::size_t>(clipId));
            df.dirtyIds.push_back(clipId);
        }
    }
}

void ECS::runInterpolationSystem() {
    auto &editState = m_buffers[m_editIndex];
    const auto &scenes = AviQtl::Core::DocumentModel::instance().scenes();
    if (scenes.empty())
        return;
    const int sceneId = scenes.front().id;

    editState.keyframeRefs.forEach([&](int clipId, const AviQtl::ECS::KeyframeRefComponent &) {
        const auto *clip = AviQtl::Core::DocumentModel::instance().findClip(sceneId, clipId);
        if (!clip)
            return;

        const int relFrame = m_currentFrame - clip->startFrame;
        auto &et = editState.ecsTransforms[clipId];

        // "transform" という特殊IDを持つエフェクトまたは標準的な補間トラックを検索
        for (const auto &effect : clip->effects) {
            if (effect.id == QStringLiteral("transform")) {
                auto eval = [&](const QString &paramName, float fallback) {
                    auto it = effect.keyframes.find(paramName);
                    if (it != effect.keyframes.end()) {
                        return AviQtl::Core::InterpolationEngine::instance().evaluate(it->second, relFrame, fallback);
                    }
                    return fallback;
                };

                et.x = eval(QStringLiteral("x"), 0.0f);
                et.y = eval(QStringLiteral("y"), 0.0f);
                et.z = eval(QStringLiteral("z"), 0.0f);
                et.scaleX = eval(QStringLiteral("scaleX"), 1.0f);
                et.scaleY = eval(QStringLiteral("scaleY"), 1.0f);
                et.rotX = eval(QStringLiteral("rotX"), 0.0f);
                et.rotY = eval(QStringLiteral("rotY"), 0.0f);
                et.rotZ = eval(QStringLiteral("rotZ"), 0.0f);
                et.opacity = eval(QStringLiteral("opacity"), 1.0f);
                break;
            }
        }
    });
}

void ECS::runTransformSystem() {
    auto &editState = m_buffers[m_editIndex];
    editState.ecsTransforms.forEach([&](int clipId, const AviQtl::ECS::TransformComponent &et) {
        auto &gm = editState.globalMatrices[clipId];

        // Z=0 等倍正投影の 4x4 アフィン行列計算 (Row-Major)
        const float rad = et.rotZ * 3.14159265f / 180.0f;
        const float c = std::cos(rad);
        const float s = std::sin(rad);

        gm.m[0] = et.scaleX * c;
        gm.m[1] = -s;
        gm.m[2] = 0.0f;
        gm.m[3] = et.x;
        gm.m[4] = s;
        gm.m[5] = et.scaleY * c;
        gm.m[6] = 0.0f;
        gm.m[7] = et.y;
        gm.m[8] = 0.0f;
        gm.m[9] = 0.0f;
        gm.m[10] = 1.0f;
        gm.m[11] = et.z;
        gm.m[12] = 0.0f;
        gm.m[13] = 0.0f;
        gm.m[14] = 0.0f;
        gm.m[15] = 1.0f;
    });
}

// ─── その他ユーティリティ ─────────────────────────────────────────────────────

auto ECS::isRenderGraphDirty() const -> bool { return m_buffers[m_editIndex].renderGraphDirty; }

void ECS::markRenderGraphClean() { m_buffers[m_editIndex].renderGraphDirty = false; }

} // namespace AviQtl::Engine::Timeline
