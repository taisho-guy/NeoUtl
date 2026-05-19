#pragma once
// ECSプロファイラ: リリースビルドでは完全にゼロコスト
// 使用方法: CMakeで -DAVIQTL_PROFILE=1 を追加するだけでよい
#ifdef AVIQTL_PROFILE
#include <QDebug>
#include <atomic>
#include <chrono>
#include <cstdint>

namespace AviQtl::Engine::Timeline {

struct ECSProfiler {
    std::atomic<uint64_t> commitCount{0};
    std::atomic<uint64_t> denseMapHit{0};
    std::atomic<uint64_t> denseMapMiss{0};
    std::atomic<uint64_t> syncAliveRemoved{0};
    std::atomic<uint64_t> dirtyBitSetCount{0};
    // フェーズ6: ECS→SSBO 直書き回数
    std::atomic<uint64_t> ssboWriteCount{0};

    static ECSProfiler &instance() {
        static ECSProfiler prof;
        return prof;
    }

    void reset() noexcept {
        commitCount.store(0, std::memory_order_relaxed);
        denseMapHit.store(0, std::memory_order_relaxed);
        denseMapMiss.store(0, std::memory_order_relaxed);
        syncAliveRemoved.store(0, std::memory_order_relaxed);
        dirtyBitSetCount.store(0, std::memory_order_relaxed);
        ssboWriteCount.store(0, std::memory_order_relaxed);
    }

    void dump() const {
        qDebug() << "[ECSProfiler]"
                 << "commit=" << commitCount.load() << "mapHit=" << denseMapHit.load() << "mapMiss=" << denseMapMiss.load() << "syncRemoved=" << syncAliveRemoved.load() << "dirtyBits=" << dirtyBitSetCount.load() << "ssboWrite=" << ssboWriteCount.load();
    }

  private:
    ECSProfiler() = default;
};

// updateActiveClipsList などの計測スコープ用RAII
struct ECSTimerScope {
    explicit ECSTimerScope(std::atomic<uint64_t> &target) : m_target(target), m_start(std::chrono::steady_clock::now()) {}
    ~ECSTimerScope() {
        auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now() - m_start).count();
        m_target.fetch_add(static_cast<uint64_t>(ns), std::memory_order_relaxed);
    }

  private:
    std::atomic<uint64_t> &m_target;
    std::chrono::steady_clock::time_point m_start;
};

} // namespace AviQtl::Engine::Timeline

#define ECS_PROF_INC(counter) AviQtl::Engine::Timeline::ECSProfiler::instance().counter.fetch_add(1, std::memory_order_relaxed)
#define ECS_PROF_ADD(counter, val) AviQtl::Engine::Timeline::ECSProfiler::instance().counter.fetch_add(static_cast<uint64_t>(val), std::memory_order_relaxed)
#define ECS_TIMER_SCOPE(counter) AviQtl::Engine::Timeline::ECSTimerScope _ecs_timer_##counter##_(AviQtl::Engine::Timeline::ECSProfiler::instance().counter)

#else
// リリースビルドでは全マクロがゼロコスト
#define ECS_PROF_INC(counter) ((void)0)
#define ECS_PROF_ADD(counter, val) ((void)0)
#define ECS_TIMER_SCOPE(counter) ((void)0)
#endif
