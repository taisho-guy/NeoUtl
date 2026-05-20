cmake_minimum_required(VERSION 4.0)
project(AviQtl LANGUAGES CXX C)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(QT_QML_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml)
set(QT_NO_PRIVATE_MODULE_WARNING ON)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(POLICY QTP0004)
    qt_policy(SET QTP0004 NEW)
endif()

# --- Dependencies ---
set(EXTRA_QT_COMPONENTS "")
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    list(APPEND EXTRA_QT_COMPONENTS WaylandClient)
endif()

find_package(Qt6 REQUIRED COMPONENTS 
    Core Gui GuiPrivate Quick QuickControls2 Quick3D 
    Widgets Core5Compat Sql Svg Multimedia Concurrent 
    ShaderTools LinguistTools Network ${EXTRA_QT_COMPONENTS}
)

if(APPLE)
    find_package(Vulkan)
else()
    find_package(Vulkan REQUIRED)
endif()

find_package(ECM REQUIRED NO_MODULE)
list(APPEND CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
find_package(PkgConfig REQUIRED)

pkg_check_modules(FFMPEG IMPORTED_TARGET REQUIRED libavcodec libavformat libavutil libswscale libswresample)
pkg_check_modules(LUAJIT IMPORTED_TARGET REQUIRED luajit)

# Carla Audio Plugin Backend
if(WIN32)
    if(NOT CARLA_SDK_DIR)
        set(CARLA_SDK_DIR "${CMAKE_SOURCE_DIR}/vendor/carla")
    else()
        file(TO_CMAKE_PATH "${CARLA_SDK_DIR}" CARLA_SDK_DIR)
    endif()
    # インクルードパスはライブラリ検出と独立して設定
    # CarlaBackend.h は source/backend/ 由来、CarlaNativePlugin.h は source/includes/ 由来
    if(EXISTS "${CARLA_SDK_DIR}/include/CarlaNativePlugin.h")
        set(CARLA_INCLUDE_DIRS "${CARLA_SDK_DIR}/include")
    endif()
    # MSYS2/MinGW では .dll を直接リンクする (.lib/.dll.a が不在のため)
    set(_carla_dll_dir "${CARLA_SDK_DIR}/lib")
    foreach(_carla_name
            libcarla_standalone2
            libcarla_native-plugin
            libcarla_host-plugin
            libcarla_utils)
        # MSVC では .lib import library を優先、他環境では .dll にフォールバック
        if(EXISTS "${_carla_dll_dir}/${_carla_name}.lib")
            list(APPEND CARLA_LIBRARIES "${_carla_dll_dir}/${_carla_name}.lib")
        elseif(EXISTS "${_carla_dll_dir}/${_carla_name}.dll")
            list(APPEND CARLA_LIBRARIES "${_carla_dll_dir}/${_carla_name}.dll")
        endif()
    endforeach()
else()
    pkg_check_modules(CARLA IMPORTED_TARGET REQUIRED carla-standalone carla-native-plugin carla-host-plugin carla-utils)
endif()

# --- Common Settings ---
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# --- Module: Core ---
file(GLOB CORE_SOURCES "core/src/*.cpp")
file(GLOB CORE_HEADERS "core/include/*.hpp")
set(EFFECT_METADATA_I18N_SOURCE "${CMAKE_SOURCE_DIR}/core/src/effect_metadata_i18n.cpp")
list(REMOVE_ITEM CORE_SOURCES "${EFFECT_METADATA_I18N_SOURCE}")
list(APPEND CORE_SOURCES "${EFFECT_METADATA_I18N_SOURCE}")
# main.cpp は後で executable に追加するため除外
list(REMOVE_ITEM CORE_SOURCES "${CMAKE_SOURCE_DIR}/core/src/main.cpp")

# UI依存ファイルを除外 (Platform Separation)
set(UI_DEPENDENT_SOURCES
    "core/src/compute_render_node.cpp"
    "core/src/video_frame_provider.cpp"
    "core/src/video_frame_store.cpp"
)
set(UI_DEPENDENT_HEADERS
    "core/include/compute_render_node.hpp"
    "core/include/video_frame_provider.hpp"
    "core/include/video_frame_store.hpp"
    "core/include/effect_model.hpp"
)

foreach(F ${UI_DEPENDENT_SOURCES})
    list(REMOVE_ITEM CORE_SOURCES "${CMAKE_SOURCE_DIR}/${F}")
endforeach()
foreach(F ${UI_DEPENDENT_HEADERS})
    list(REMOVE_ITEM CORE_HEADERS "${CMAKE_SOURCE_DIR}/${F}")
endforeach()
# QMLモジュール側で処理されるファイルを個別に除外
list(REMOVE_ITEM CORE_SOURCES "${CMAKE_SOURCE_DIR}/core/src/compute_effect.cpp")
list(REMOVE_ITEM CORE_HEADERS "${CMAKE_SOURCE_DIR}/core/include/compute_effect.hpp")

add_library(AviQtl_Core STATIC ${CORE_SOURCES} ${CORE_HEADERS})
target_include_directories(AviQtl_Core PUBLIC core/include ${CMAKE_SOURCE_DIR})
target_link_libraries(AviQtl_Core PUBLIC Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Concurrent Qt6::Network Qt6::Multimedia Qt6::Qml PkgConfig::FFMPEG PkgConfig::LUAJIT)

# --- Module: Engine ---
file(GLOB_RECURSE ENGINE_SOURCES "engine/*.cpp")
file(GLOB_RECURSE ENGINE_HEADERS "engine/*.hpp")
add_library(AviQtl_Engine STATIC ${ENGINE_SOURCES} ${ENGINE_HEADERS})
target_include_directories(AviQtl_Engine PUBLIC engine ${CARLA_INCLUDE_DIRS})
if(WIN32)
    target_link_libraries(AviQtl_Engine PUBLIC AviQtl_Core Qt6::Qml Qt6::Quick ${CARLA_LIBRARIES})
else()
    target_link_libraries(AviQtl_Engine PUBLIC AviQtl_Core Qt6::Qml Qt6::Quick PkgConfig::CARLA)
endif()

# --- Module: Scripting ---
file(GLOB SCRIPTING_SOURCES "scripting/*.cpp")
file(GLOB SCRIPTING_HEADERS "scripting/*.hpp")
add_library(AviQtl_Scripting STATIC ${SCRIPTING_SOURCES} ${SCRIPTING_HEADERS})
target_include_directories(AviQtl_Scripting PUBLIC scripting)
target_link_libraries(AviQtl_Scripting PUBLIC AviQtl_Core Qt6::Qml Qt6::Quick PkgConfig::LUAJIT)

# --- Module: UI (Bridge & Controllers) ---
file(GLOB_RECURSE UI_SOURCES "ui/src/*.cpp")
file(GLOB_RECURSE UI_HEADERS "ui/include/*.hpp")
# Core から移設した UI 依存ファイルを統合
list(APPEND UI_SOURCES ${UI_DEPENDENT_SOURCES})
list(APPEND UI_HEADERS ${UI_DEPENDENT_HEADERS})

add_library(AviQtl_UI STATIC ${UI_SOURCES} ${UI_HEADERS})
target_include_directories(AviQtl_UI PUBLIC ui/include)
target_link_libraries(AviQtl_UI PUBLIC AviQtl_Engine AviQtl_Scripting Qt6::Quick Qt6::QuickControls2 Qt6::Quick3D Qt6::Multimedia Qt6::Svg Qt6::Core5Compat Qt6::GuiPrivate Vulkan::Vulkan)

# --- QML Resources & Module ---
# effects と objects は外部モジュールとして実行時に読み込むため、QMLモジュール（バイナリ埋め込み）から除外
file(GLOB_RECURSE QML_FILES RELATIVE ${CMAKE_CURRENT_SOURCE_DIR} 
    "ui/qml/*.qml" 
    "ui/qml/*.js"
)
list(FILTER QML_FILES EXCLUDE REGEX "ui/qml/(effects|objects)/.*")

# 共通シェーダーのビルド
file(GLOB_RECURSE COMMON_SHADERS RELATIVE ${CMAKE_CURRENT_SOURCE_DIR} "ui/qml/common/shaders/*.glsl")

# --- QML Module (Attached to UI Module) ---
qt_add_qml_module(AviQtl_UI
    URI "AviQtl"
    VERSION 1.0
    RESOURCE_PREFIX "/qt/qml"
    SOURCES
        "core/src/compute_effect.cpp"
        "core/include/compute_effect.hpp"
    QML_FILES
        ${QML_FILES}
)

# --- External Assets: Shaders Compilation ---
# effects と objects のシェーダーは .qsb にコンパイルして BUILD.py が拾えるようにする
# バイナリに埋め込まず、ファイルとして出力する
set(EXTERNAL_QSB_FILES "")
foreach(DIR "effects" "objects")
    file(GLOB_RECURSE SHADERS RELATIVE ${CMAKE_CURRENT_SOURCE_DIR} "ui/qml/${DIR}/*.frag" "ui/qml/${DIR}/*.vert" "ui/qml/${DIR}/*.comp")
    foreach(SH ${SHADERS})
        get_filename_component(SH_FILE_NAME ${SH} NAME)
        get_filename_component(SH_EXT ${SH} EXT)
        
        set(OUTPUT_QSB "${CMAKE_BINARY_DIR}/${DIR}/${SH_FILE_NAME}.qsb")
        list(APPEND EXTERNAL_QSB_FILES ${OUTPUT_QSB})

        set(QSB_ARGS "--glsl" "100es,120,150" "--hlsl" "50" "--msl" "12")
        if(SH_EXT STREQUAL ".comp")
            set(QSB_ARGS "--glsl" "310es,430" "--hlsl" "50" "--msl" "12")
        endif()

        add_custom_command(
            OUTPUT ${OUTPUT_QSB}
            COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_BINARY_DIR}/${DIR}"
            COMMAND Qt6::qsb ${QSB_ARGS} -o ${OUTPUT_QSB} "${CMAKE_SOURCE_DIR}/${SH}"
            DEPENDS "${CMAKE_SOURCE_DIR}/${SH}"
            VERBATIM
        )
    endforeach()
endforeach()

# シェーダー生成をビルドプロセスに含めるためのダミーターゲット
add_custom_target(AviQtl_ExternalShaders ALL DEPENDS ${EXTERNAL_QSB_FILES})

# --- Executable ---
qt_add_executable(AviQtl
    core/src/main.cpp
    ui/resources.qrc
    ${QML_FILES}
    ${COMMON_SHADERS}
)

if(APPLE)
    set_target_properties(AviQtl PROPERTIES
        MACOSX_BUNDLE TRUE
        MACOSX_BUNDLE_BUNDLE_NAME "AviQtl"
        MACOSX_BUNDLE_GUI_IDENTIFIER "org.aviqtl.AviQtl"
        MACOSX_BUNDLE_BUNDLE_VERSION "${PROJECT_VERSION}"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "${PROJECT_VERSION}"
    )
endif()

# 外部シェーダー生成の依存関係を登録
add_dependencies(AviQtl AviQtl_ExternalShaders)

target_link_libraries(AviQtl PRIVATE AviQtl_UI)

# --- Translation ---
file(GLOB TS_FILES "i18n/AviQtl_*.ts")
if(TS_FILES)
    qt_add_translations(AviQtl TS_FILES ${TS_FILES})
    add_dependencies(AviQtl AviQtl_lupdate AviQtl_lrelease)
    set(QM_FILES "")
    foreach(TS_FILE ${TS_FILES})
        get_filename_component(TS_NAME "${TS_FILE}" NAME_WE)
        list(APPEND QM_FILES "${CMAKE_BINARY_DIR}/${TS_NAME}.qm")
    endforeach()
    if(APPLE)
        set(AVIQTL_I18N_OUTPUT_DIR "$<TARGET_BUNDLE_DIR:AviQtl>/Contents/Resources/i18n")
    else()
        set(AVIQTL_I18N_OUTPUT_DIR "$<TARGET_FILE_DIR:AviQtl>/i18n")
    endif()
    add_custom_command(TARGET AviQtl POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E make_directory "${AVIQTL_I18N_OUTPUT_DIR}"
        COMMAND ${CMAKE_COMMAND} -E copy_if_different ${QM_FILES} "${AVIQTL_I18N_OUTPUT_DIR}"
        VERBATIM
    )
endif()

# --- Platform Specific ---
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    target_link_options(AviQtl PRIVATE "-rdynamic")
    target_link_libraries(AviQtl PRIVATE Qt6::WaylandClient)
elseif(MSVC)
    target_link_libraries(AviQtl PRIVATE opengl32)
    set_target_properties(AviQtl PROPERTIES MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()

# --- Compile Options ---
target_compile_options(AviQtl PRIVATE
    $<$<OR:$<CXX_COMPILER_ID:Clang>,$<CXX_COMPILER_ID:GNU>>: -Wall -Wextra -Wpedantic -Wno-unused-parameter>
    $<$<CXX_COMPILER_ID:Clang>: -fcolor-diagnostics -Wno-nullability-extension -flto=thin -Wno-deprecated-declarations -Wno-non-virtual-dtor>
    $<$<CXX_COMPILER_ID:MSVC>: /W4 /permissive- /utf-8>
)
target_link_options(AviQtl PRIVATE
    $<$<AND:$<CXX_COMPILER_ID:Clang>,$<PLATFORM_ID:Linux>>: -flto=thin>
    $<$<AND:$<CXX_COMPILER_ID:MSVC>,$<CONFIG:Release>>: /LTCG>
)

# --- Linker selection ---
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND NOT DEFINED CMAKE_LINKER_TYPE)
    find_program(MOLD_LINKER mold)
    if(MOLD_LINKER)
        set(CMAKE_LINKER_TYPE MOLD CACHE STRING "" FORCE)
    endif()
endif()
