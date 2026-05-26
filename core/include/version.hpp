#pragma once

namespace AviQtl {
// These values will be defined by CMake.
// If not defined (e.g., for IDE builds without proper CMake setup, or if BUILD.py doesn't pass --version),
// they will default to 0.0.0 and "Unstable".
#ifndef AVIQTL_VERSION_MAJOR
constexpr int VERSION_MAJOR = 0;
#else
constexpr int VERSION_MAJOR = AVIQTL_VERSION_MAJOR;
#endif

#ifndef AVIQTL_VERSION_MINOR
constexpr int VERSION_MINOR = 0;
#else
constexpr int VERSION_MINOR = AVIQTL_VERSION_MINOR;
#endif

#ifndef AVIQTL_VERSION_PATCH
constexpr int VERSION_PATCH = 0;
#else
constexpr int VERSION_PATCH = AVIQTL_VERSION_PATCH;
#endif

#ifndef AVIQTL_VERSION_STRING
constexpr const char *VERSION_STRING = "0.0.0";
#else
constexpr const char *VERSION_STRING = AVIQTL_VERSION_STRING;
#endif

#ifndef AVIQTL_VERSION_CODENAME
constexpr const char *VERSION_CODENAME = "Unstable";
#else
constexpr const char *VERSION_CODENAME = AVIQTL_VERSION_CODENAME;
#endif
} // namespace AviQtl
