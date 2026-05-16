#pragma once
#include "ecs_profiler.hpp"
// PODコンポーネント定義 (仕様書第4章準拠)
// 全フィールドは trivially_copyable なスカラー値のみ。
// ポインタ・std::string・QString・QListの混入を厳禁とする。
#include <cstdint>

namespace AviQtl::ECS {

struct ActiveComponent {
    bool active = false;
};
struct TransformComponent {
    float x = 0, y = 0, z = 0, scaleX = 1, scaleY = 1, rotX = 0, rotY = 0, rotZ = 0, opacity = 1;
};
struct KeyframeRefComponent {
    uint32_t clipId = 0;
    uint32_t effectId = 0;
};
struct RenderableComponent {
    uint32_t textureId = 0;
    uint32_t materialId = 0;
    uint32_t layer = 0;
};
struct RenderBoundaryComponent {
    bool clearBelow = false;
    uint32_t layer = 0;
};
struct GroupTransformComponent {
    uint32_t layerCount = 1;
};
struct GlobalMatrixComponent {
    float m[16] = {1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1};
};

} // namespace AviQtl::ECS

#include "ssbo_layout.hpp"
#include "string_table.hpp"
#include <QString>
#include <array>
#include <atomic>
#include <bitset>
#include <cassert>
#include <cstddef>
#include <iterator>
#include <utility>
#include <vector>

// Phase 2.4: CoreBridge の前方宣言 (循環インクルード回避)
namespace AviQtl::UI {
class CoreBridge;
}

namespace AviQtl::Engine::Timeline {

inline constexpr int MAX_CLIP_ID = 4096;

struct DirtyFlags {
    std::bitset<MAX_CLIP_ID> dirty;
    std::vector<int> dirtyIds; // 最適化: 変更された ID を直接保持
    bool fullSync = false;
};

template <typename T> class DenseComponentMap {
  public:
    T &operator[](int clipId) {
        ensureSparseSize(clipId);
        int &denseIndex = m_sparse[static_cast<std::size_t>(clipId)];
        if (denseIndex >= 0) {
            ECS_PROF_INC(denseMapHit);
            return m_data[static_cast<std::size_t>(denseIndex)];
        }
        ECS_PROF_INC(denseMapMiss);
        denseIndex = static_cast<int>(m_data.size());
        m_entities.push_back(clipId);
        m_data.push_back(T{});
        return m_data.back();
    }

    T *find(int clipId) {
        if (clipId < 0 || clipId >= static_cast<int>(m_sparse.size()))
            return nullptr;
        const int denseIndex = m_sparse[static_cast<std::size_t>(clipId)];
        if (denseIndex < 0)
            return nullptr;
        return &m_data[static_cast<std::size_t>(denseIndex)];
    }
    const T *find(int clipId) const { return const_cast<DenseComponentMap *>(this)->find(clipId); }

    void erase(int clipId) {
        if (clipId < 0 || clipId >= static_cast<int>(m_sparse.size()))
            return;
        int denseIndex = m_sparse[static_cast<std::size_t>(clipId)];
        if (denseIndex < 0)
            return;
        int lastIndex = static_cast<int>(m_data.size()) - 1;
        if (denseIndex != lastIndex) {
            m_data[static_cast<std::size_t>(denseIndex)] = std::move(m_data[static_cast<std::size_t>(lastIndex)]);
            int movedClipId = m_entities[static_cast<std::size_t>(lastIndex)];
            m_entities[static_cast<std::size_t>(denseIndex)] = movedClipId;
            m_sparse[static_cast<std::size_t>(movedClipId)] = denseIndex;
        }
        m_data.pop_back();
        m_entities.pop_back();
        m_sparse[static_cast<std::size_t>(clipId)] = -1;
    }

    bool syncAlive(const std::bitset<MAX_CLIP_ID> &aliveFlags) {
        bool changed = false;
        for (int i = static_cast<int>(m_entities.size()) - 1; i >= 0; --i) {
            int id = m_entities[static_cast<std::size_t>(i)];
            if (id >= MAX_CLIP_ID || !aliveFlags.test(static_cast<std::size_t>(id))) {
                erase(id);
                changed = true;
                ECS_PROF_INC(syncAliveRemoved);
            }
        }
        return changed;
    }

    using iterator = T *;
    using const_iterator = const T *;
    iterator begin() { return m_data.empty() ? nullptr : &m_data[0]; }
    iterator end() { return m_data.empty() ? nullptr : &m_data[0] + m_data.size(); }
    const_iterator begin() const { return m_data.empty() ? nullptr : &m_data[0]; }
    const_iterator end() const { return m_data.empty() ? nullptr : &m_data[0] + m_data.size(); }

    bool contains(int clipId) const {
        if (clipId < 0 || clipId >= static_cast<int>(m_sparse.size()))
            return false;
        return m_sparse[static_cast<std::size_t>(clipId)] != -1;
    }

    template <typename Fn> void forEach(Fn &&fn) const {
        for (std::size_t i = 0; i < m_data.size(); ++i)
            fn(m_entities[i], m_data[i]);
    }

  private:
    void ensureSparseSize(int clipId) {
        if (clipId < 0)
            return;
        const std::size_t needed = static_cast<std::size_t>(clipId) + 1;
        if (m_sparse.size() < needed)
            m_sparse.resize(needed, -1);
    }
    std::vector<int> m_entities;
    std::vector<T> m_data;
    std::vector<int> m_sparse;
};

struct AudioComponent {
    int clipId = -1;
    int startFrame = 0;
    int durationFrames = 0;
    float volume = 1.0f;
    float pan = 0.0f;
    bool mute = false;
};

struct TransformComponent {
    int layer = 0;
    double timePosition = 0.0;
    int startFrame = 0;
    int durationFrames = 0;
};

struct MetadataComponent {
    int32_t clipId = -1;
    uint32_t nameId = 0;
    uint32_t sourceId = 0;
    uint32_t typeId = 0;
    uint32_t colorRGBA = 0;
};
static_assert(sizeof(MetadataComponent) == 20, "MetadataComponent size check failed");
static_assert(std::is_trivially_copyable_v<MetadataComponent>);

struct RenderComponent {
    bool needsUpdate = true;
    uint32_t effectChainId = 0;
};
static_assert(std::is_trivially_copyable_v<RenderComponent>);

struct ECSState {
    bool renderGraphDirty = false;
    DenseComponentMap<TransformComponent> transforms;
    DenseComponentMap<RenderComponent> renderStates;
    DenseComponentMap<AudioComponent> audioStates;
    DenseComponentMap<MetadataComponent> metadataStates;
};

class ECS {
  public:
    static ECS &instance();

    // ── Phase 2.4: CommandSystem ──────────────────────────────────────────────
    // CoreBridge の SPSC リングバッファからコマンドを取り出し ECS 状態に反映する。
    // TimelineController の毎フレーム先頭 (onTick 相当) から呼び出すこと。
    void runCommandSystem(AviQtl::UI::CoreBridge &bridge);

    int currentFrame() const { return m_currentFrame; }
    bool isPlaying() const { return m_isPlaying; }
    // ─────────────────────────────────────────────────────────────────────────

    void syncClipIds(const std::bitset<MAX_CLIP_ID> &aliveFlags);
    void updateClipState(int clipId, int layer, double time, int startFrame, int durationFrames);
    void updateAudioClipState(int clipId, int startFrame, int durationFrames, float volume, float pan, bool mute);
    void updateMetadata(int clipId, const QString &name, const QString &source, const QString &type, const QString &color);

    void commit();

    void writeSSBOLayout(GpuClipSoA &out) const;

    ECSState &editState() { return m_buffers[m_editIndex]; }

    void markEvaluatedParamsDirty(int clipId) {
        assert(clipId >= 0 && clipId < MAX_CLIP_ID);
        m_dirtyFlags[(m_editIndex + 1) % 3].dirty.set(static_cast<std::size_t>(clipId));
        m_dirtyFlags[(m_editIndex + 2) % 3].dirty.set(static_cast<std::size_t>(clipId));
    }

    const ECSState *getSnapshot() const;

    const StringTable &stringTable() const { return m_stringTable; }

    bool isRenderGraphDirty() const;
    void markRenderGraphClean();

  private:
    ECS();

    std::array<ECSState, 3> m_buffers;
    int m_editIndex = 0;
    mutable std::atomic<int> m_activeIndex{0};
    mutable std::atomic<int> m_pendingIndex{-1};

    StringTable m_stringTable;
    std::array<DirtyFlags, 3> m_dirtyFlags;

    // Phase 2.4: CommandSystem が管理するグローバル再生状態
    int m_currentFrame = 0;
    bool m_isPlaying = false;
};

} // namespace AviQtl::Engine::Timeline
