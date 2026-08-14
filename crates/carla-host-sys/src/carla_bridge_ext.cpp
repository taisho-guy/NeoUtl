#include "CarlaDefines.h"
#include "CarlaBackend.h"
#include "CarlaEngine.hpp"
#include "CarlaPlugin.hpp"
#include "CarlaHost.h"
#include "CarlaHostImpl.hpp"
#include "backend/utils/CachedPlugins.cpp"
#include <cstring>
#include <algorithm>
#include <unordered_map>
#include <vector>
#include <mutex>

#if !defined(_WIN32)
#include <dlfcn.h>
#include "clap/entry.h"
#include "clap/plugin.h"
#include "clap/plugin-factory.h"
#include "clap/ext/gui.h"

// ------------------------------------------------------------------------------------------------
// Safe CLAP GUI Extension Wrapper
// Prevents DPF and other CLAP plugins from asserting / crashing when can_resize is called before create()

static std::mutex gClapMutex;
static std::unordered_map<const clap_plugin_t*, bool> gGuiCreated;
static std::unordered_map<const clap_plugin_t*, const clap_plugin_gui_t*> gRealGuiExts;
static std::unordered_map<const clap_plugin_t*, clap_plugin_gui_t> gWrappedGuiExts;
static std::unordered_map<const clap_plugin_t*, const void* (*)(const clap_plugin_t*, const char*)> gRealGetExts;
static std::unordered_map<const clap_plugin_factory_t*, const clap_plugin_factory_t*> gRealFactories;
static std::unordered_map<const clap_plugin_factory_t*, clap_plugin_factory_t> gWrappedFactories;
static std::unordered_map<const clap_plugin_entry_t*, const clap_plugin_entry_t*> gRealEntries;
static std::unordered_map<const clap_plugin_entry_t*, clap_plugin_entry_t> gWrappedEntries;

static bool safe_clap_gui_can_resize(const clap_plugin_t *plugin) {
    if (plugin == nullptr) return true;
    std::lock_guard<std::mutex> lock(gClapMutex);
    auto it = gGuiCreated.find(plugin);
    if (it == gGuiCreated.end() || !it->second) {
        return true; // Not created yet: return safe default
    }
    auto git = gRealGuiExts.find(plugin);
    if (git != gRealGuiExts.end() && git->second && git->second->can_resize) {
        return git->second->can_resize(plugin);
    }
    return true;
}

static bool safe_clap_gui_create(const clap_plugin_t *plugin, const char *api, bool is_floating) {
    if (plugin == nullptr) return false;
    bool res = false;
    {
        std::lock_guard<std::mutex> lock(gClapMutex);
        auto git = gRealGuiExts.find(plugin);
        if (git != gRealGuiExts.end() && git->second && git->second->create) {
            res = git->second->create(plugin, api, is_floating);
        }
        if (res) {
            gGuiCreated[plugin] = true;
        }
    }
    return res;
}

static void safe_clap_gui_destroy(const clap_plugin_t *plugin) {
    if (plugin == nullptr) return;
    const clap_plugin_gui_t* realGui = nullptr;
    {
        std::lock_guard<std::mutex> lock(gClapMutex);
        gGuiCreated[plugin] = false;
        auto git = gRealGuiExts.find(plugin);
        if (git != gRealGuiExts.end()) {
            realGui = git->second;
        }
    }
    if (realGui && realGui->destroy) {
        realGui->destroy(plugin);
    }
}

static const void* safe_clap_get_extension(const clap_plugin_t *plugin, const char *id) {
    if (plugin == nullptr || id == nullptr) return nullptr;
    const void* (*real_get_ext)(const clap_plugin_t*, const char*) = nullptr;
    {
        std::lock_guard<std::mutex> lock(gClapMutex);
        auto it = gRealGetExts.find(plugin);
        if (it != gRealGetExts.end()) {
            real_get_ext = it->second;
        }
    }
    if (!real_get_ext) return nullptr;
    const void* ext = real_get_ext(plugin, id);
    if (!ext) return nullptr;

    if (std::strcmp(id, CLAP_EXT_GUI) == 0) {
        std::lock_guard<std::mutex> lock(gClapMutex);
        const clap_plugin_gui_t* realGui = static_cast<const clap_plugin_gui_t*>(ext);
        gRealGuiExts[plugin] = realGui;
        clap_plugin_gui_t& wrapped = gWrappedGuiExts[plugin];
        wrapped = *realGui;
        wrapped.can_resize = safe_clap_gui_can_resize;
        wrapped.create = safe_clap_gui_create;
        wrapped.destroy = safe_clap_gui_destroy;
        return &wrapped;
    }
    return ext;
}

static const clap_plugin_t* safe_clap_create_plugin(const clap_plugin_factory_t *factory, const clap_host_t *host, const char *plugin_id) {
    if (factory == nullptr) return nullptr;
    const clap_plugin_factory_t* realFactory = factory;
    {
        std::lock_guard<std::mutex> lock(gClapMutex);
        auto it = gRealFactories.find(factory);
        if (it != gRealFactories.end()) {
            realFactory = it->second;
        }
    }
    if (!realFactory || !realFactory->create_plugin) return nullptr;
    const clap_plugin_t* realPlugin = realFactory->create_plugin(realFactory, host, plugin_id);
    if (!realPlugin) return nullptr;

    std::lock_guard<std::mutex> lock(gClapMutex);
    gRealGetExts[realPlugin] = realPlugin->get_extension;
    const_cast<clap_plugin_t*>(realPlugin)->get_extension = safe_clap_get_extension;
    return realPlugin;
}

static const void* safe_clap_entry_get_factory(const char *factory_id) {
    std::lock_guard<std::mutex> lock(gClapMutex);
    for (const auto& kv : gRealEntries) {
        if (kv.second && kv.second->get_factory) {
            const void* fac = kv.second->get_factory(factory_id);
            if (fac) {
                if (factory_id != nullptr && std::strcmp(factory_id, CLAP_PLUGIN_FACTORY_ID) == 0) {
                    const clap_plugin_factory_t* realFac = static_cast<const clap_plugin_factory_t*>(fac);
                    clap_plugin_factory_t& wrapped = gWrappedFactories[realFac];
                    wrapped = *realFac;
                    wrapped.create_plugin = safe_clap_create_plugin;
                    gRealFactories[&wrapped] = realFac;
                    return &wrapped;
                }
                return fac;
            }
        }
    }
    return nullptr;
}

static const void* wrap_clap_entry(const clap_plugin_entry_t* realEntry) {
    if (!realEntry) return nullptr;
    std::lock_guard<std::mutex> lock(gClapMutex);
    auto it = gWrappedEntries.find(realEntry);
    if (it != gWrappedEntries.end()) {
        return &it->second;
    }
    clap_plugin_entry_t& wrapped = gWrappedEntries[realEntry];
    wrapped = *realEntry;
    wrapped.get_factory = safe_clap_entry_get_factory;
    gRealEntries[&wrapped] = realEntry;
    return &wrapped;
}

extern "C" {

void* dlsym(void* handle, const char* name) {
    static void* (*real_dlsym_fn)(void*, const char*) = nullptr;
    if (!real_dlsym_fn) {
        #if defined(RTLD_NEXT)
        real_dlsym_fn = (void* (*)(void*, const char*))dlvsym(RTLD_NEXT, "dlsym", "GLIBC_2.2.5");
        if (!real_dlsym_fn) {
            real_dlsym_fn = (void* (*)(void*, const char*))dlvsym(RTLD_NEXT, "dlsym", "GLIBC_2.34");
        }
        #endif
    }
    if (!real_dlsym_fn) return nullptr;

    void* sym = real_dlsym_fn(handle, name);
    if (sym != nullptr && std::strcmp(name, "clap_entry") == 0) {
        return const_cast<void*>(wrap_clap_entry(static_cast<const clap_plugin_entry_t*>(sym)));
    }
    return sym;
}

}
#endif

extern "C" {

CARLA_API_EXPORT void carla_plugin_process_stereo(
    CarlaHostHandle handle,
    uint32_t pluginId,
    const float* inL,
    const float* inR,
    float* outL,
    float* outR,
    uint32_t frames
) {
    if (frames == 0) return;
    if (outL == nullptr || outR == nullptr) return;

    if (handle == nullptr || handle->engine == nullptr) {
        if (inL && outL && inL != outL) std::memcpy(outL, inL, sizeof(float) * frames);
        if (inR && outR && inR != outR) std::memcpy(outR, inR, sizeof(float) * frames);
        return;
    }

    CarlaBackend::CarlaEngine* const engine = handle->engine;
    CarlaBackend::CarlaPluginPtr plugin = engine->getPlugin(pluginId);
    if (!plugin || !plugin->isEnabled()) {
        if (inL && outL && inL != outL) std::memcpy(outL, inL, sizeof(float) * frames);
        if (inR && outR && inR != outR) std::memcpy(outR, inR, sizeof(float) * frames);
        return;
    }

    if (!plugin->tryLock(true)) {
        if (inL && outL && inL != outL) std::memcpy(outL, inL, sizeof(float) * frames);
        if (inR && outR && inR != outR) std::memcpy(outR, inR, sizeof(float) * frames);
        return;
    }

    plugin->initBuffers();

    const uint32_t inCount = plugin->getAudioInCount();
    const uint32_t outCount = plugin->getAudioOutCount();

    if (inCount == 0 && outCount == 0) {
        plugin->process(nullptr, nullptr, nullptr, nullptr, frames);
        plugin->unlock();
        return;
    }

    constexpr uint32_t kMaxChannels = 64;
    const uint32_t safeInCount = std::min<uint32_t>(std::max<uint32_t>(2, inCount), kMaxChannels);
    const uint32_t safeOutCount = std::min<uint32_t>(std::max<uint32_t>(2, outCount), kMaxChannels);

    static thread_local std::vector<float> dummyBuffer;
    if (dummyBuffer.size() < frames) {
        dummyBuffer.assign(frames, 0.0f);
    } else {
        std::memset(dummyBuffer.data(), 0, sizeof(float) * frames);
    }

    const float* inBufs[kMaxChannels];
    float* outBufs[kMaxChannels];

    inBufs[0] = inL ? inL : dummyBuffer.data();
    inBufs[1] = inR ? inR : inBufs[0];
    for (uint32_t i = 2; i < safeInCount; ++i) {
        inBufs[i] = dummyBuffer.data();
    }

    outBufs[0] = outL;
    outBufs[1] = outR;
    for (uint32_t i = 2; i < safeOutCount; ++i) {
        outBufs[i] = dummyBuffer.data();
    }

    plugin->process(
        safeInCount > 0 ? inBufs : nullptr,
        safeOutCount > 0 ? outBufs : nullptr,
        nullptr, nullptr, frames
    );

    if (outCount == 1) {
        std::memcpy(outR, outL, sizeof(float) * frames);
    }

    plugin->unlock();
}

CARLA_API_EXPORT void carla_plugin_process(
    CarlaHostHandle handle,
    uint32_t pluginId,
    const float* const* audioIn,
    float* const* audioOut,
    uint32_t frames
) {
    if (frames == 0) return;
    if (handle == nullptr || handle->engine == nullptr) return;
    CarlaBackend::CarlaEngine* const engine = handle->engine;
    CarlaBackend::CarlaPluginPtr plugin = engine->getPlugin(pluginId);
    if (!plugin || !plugin->isEnabled()) return;

    if (!plugin->tryLock(true)) return;

    plugin->initBuffers();
    plugin->process(audioIn, (float**)audioOut, nullptr, nullptr, frames);
    plugin->unlock();
}

}
