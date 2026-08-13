#ifndef CARLA_HOST_SYS_WRAPPER_H
#define CARLA_HOST_SYS_WRAPPER_H

#include "CarlaDefines.h"
#include "CarlaBackend.h"
#include "CarlaHost.h"
#include "CarlaUtils.h"

#ifdef __cplusplus
extern "C" {
#endif

CARLA_API_EXPORT void carla_plugin_process_stereo(
    CarlaHostHandle handle,
    uint32_t pluginId,
    const float* inL,
    const float* inR,
    float* outL,
    float* outR,
    uint32_t frames
);

CARLA_API_EXPORT void carla_plugin_process(
    CarlaHostHandle handle,
    uint32_t pluginId,
    const float* const* audioIn,
    float* const* audioOut,
    uint32_t frames
);

#ifdef __cplusplus
}
#endif

#endif // CARLA_HOST_SYS_WRAPPER_H
