#include "ecs.hpp"
#include "ecs_profiler.hpp"
#include "engine/plugin/audio_plugin_manager.hpp"
#include "ui/include/bridge/core_bridge.hpp"
#include <QDebug>
#include <cassert>
#include <cmath>

namespace AviQtl::Engine::Timeline {

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

ECS::ECS() : m_editIndex(1) {
    m_activeIndex.store(0, std::memory_order_relaxed);
    for (auto &f : m_dirtyFlags)
        f.fullSync = true;
}

auto ECS::instance() -> ECS & {
    static ECS inst;
    return inst;
}

void ECS::syncClipIds(const std::bitset<MAX_CLIP_ID> &aliveFlags) {
    auto &editState = m_buffers[m_editIndex];
    bool changed = false;
    changed |= editState.transforms.syncAlive(aliveFlags);
    changed |= editState.renderStates.syncAlive(aliveFlags);
    changed |= editState.audioStates.syncAlive(aliveFlags);

    changed |= editState.keyframeRefs.syncAlive(aliveFlags);
    changed |= editState.ecsTransforms.syncAlive(aliveFlags);
    changed |= editState.globalMatrices.syncAlive(aliveFlags);

    if (changed) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
    }
}

void ECS::updateClipState(int clipId, int layer, double time, int startFrame, int durationFrames) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    auto *ptr = editState.transforms.find(clipId);
    if (!ptr) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
        ptr = &editState.transforms[clipId];
    }
    auto &transform = *ptr;
    bool changed = (transform.layer != layer) || (std::abs(transform.timePosition - time) > 0.001) || (transform.startFrame != startFrame) || (transform.durationFrames != durationFrames);
    if (changed) {
        transform.layer = layer;
        transform.timePosition = time;
        transform.startFrame = startFrame;
        transform.durationFrames = durationFrames;
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

void ECS::updateAudioClipState(int clipId, int startFrame, int durationFrames, float volume, float pan, bool mute) {
    assert(clipId >= 0 && clipId < MAX_CLIP_ID);
    auto &editState = m_buffers[m_editIndex];
    auto *ptr = editState.audioStates.find(clipId);
    if (!ptr) {
        m_dirtyFlags[(m_editIndex + 1) % 3].fullSync = true;
        m_dirtyFlags[(m_editIndex + 2) % 3].fullSync = true;
        ptr = &editState.audioStates[clipId];
    }
    auto &audio = *ptr;
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

auto ECS::getSnapshot() const -> const ECSState * {
    int pending = m_pendingIndex.load(std::memory_order_acquire);
    if (pending != -1) {
        if (m_pendingIndex.compare_exchange_strong(pending, -1, std::memory_order_acq_rel, std::memory_order_relaxed)) {
            m_activeIndex.store(pending, std::memory_order_release);
        }
    }
    return &m_buffers[m_activeIndex.load(std::memory_order_acquire)];
}

auto ECS::isRenderGraphDirty() const -> bool { return m_buffers[m_editIndex].renderGraphDirty; }

void ECS::markRenderGraphClean() { m_buffers[m_editIndex].renderGraphDirty = false; }

} // namespace AviQtl::Engine::Timeline
