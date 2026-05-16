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

// ─── その他ユーティリティ ─────────────────────────────────────────────────────

auto ECS::isRenderGraphDirty() const -> bool { return m_buffers[m_editIndex].renderGraphDirty; }

void ECS::markRenderGraphClean() { m_buffers[m_editIndex].renderGraphDirty = false; }

} // namespace AviQtl::Engine::Timeline
