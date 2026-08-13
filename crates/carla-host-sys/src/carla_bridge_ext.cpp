#include "CarlaDefines.h"
#include "CarlaBackend.h"
#include "CarlaEngine.hpp"
#include "CarlaPlugin.hpp"
#include "CarlaHost.h"
#include "CarlaHostImpl.hpp"

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
    if (handle == nullptr || handle->engine == nullptr) return;
    CarlaBackend::CarlaEngine* const engine = handle->engine;
    CarlaBackend::CarlaPluginPtr plugin = engine->getPlugin(pluginId);
    if (!plugin) return;

    const float* audioIn[2] = { inL, inR };
    float* audioOut[2] = { outL, outR };
    plugin->process(audioIn, audioOut, nullptr, nullptr, frames);
}

CARLA_API_EXPORT void carla_plugin_process(
    CarlaHostHandle handle,
    uint32_t pluginId,
    const float* const* audioIn,
    float* const* audioOut,
    uint32_t frames
) {
    if (handle == nullptr || handle->engine == nullptr) return;
    CarlaBackend::CarlaEngine* const engine = handle->engine;
    CarlaBackend::CarlaPluginPtr plugin = engine->getPlugin(pluginId);
    if (!plugin) return;

    plugin->process(audioIn, (float**)audioOut, nullptr, nullptr, frames);
}

}
