#include "mod_engine.hpp"
#include "../ui/include/timeline_controller.hpp"
#include <QCoreApplication>
#include <QDebug>
#include <QMetaObject>
#include <QVariant>

namespace AviQtl::Scripting {

// Lua から参照できるグローバルポインタ
static AviQtl::UI::TimelineController *g_ctrl = nullptr;

// C API Wrappers for HostApiTable
extern "C" {
static void api_log(const char *msg) {
    if (g_ctrl != nullptr) {
        AviQtl::UI::TimelineController::log(QString::fromUtf8(msg));
    }
}
static void api_transport_play() {
    if ((g_ctrl != nullptr) && !g_ctrl->transport()->isPlaying()) {
        g_ctrl->transport()->togglePlay();
    }
}
static void api_transport_pause() {
    if ((g_ctrl != nullptr) && g_ctrl->transport()->isPlaying()) {
        g_ctrl->transport()->togglePlay();
    }
}
static void api_transport_toggle() {
    if (g_ctrl != nullptr) {
        g_ctrl->transport()->togglePlay();
    }
}
static void api_transport_seek(int frame) {
    if (g_ctrl != nullptr) {
        g_ctrl->transport()->setCurrentFrame(frame);
    }
}
static auto api_transport_get_frame() -> int { return (g_ctrl != nullptr) ? g_ctrl->transport()->currentFrame() : 0; }
static auto api_transport_is_playing() -> int { return (g_ctrl != nullptr) ? (int)g_ctrl->transport()->isPlaying() : 0; }

static void api_clip_create(const char *type, int start, int layer) {
    if (g_ctrl != nullptr) {
        g_ctrl->createObject(QString::fromUtf8(type), start, layer);
    }
}
static void api_clip_delete(int id) {
    if (g_ctrl != nullptr) {
        g_ctrl->deleteClip(id);
    }
}
static void api_clip_update(int id, int layer, int start, int dur) {
    if (g_ctrl != nullptr) {
        g_ctrl->updateClip(id, layer, start, dur);
    }
}
static void api_clip_select(int id) {
    if (g_ctrl != nullptr) {
        g_ctrl->selectClip(id);
    }
}

static auto api_project_get_width() -> int { return (g_ctrl != nullptr) ? g_ctrl->project()->width() : 0; }
static auto api_project_get_height() -> int { return (g_ctrl != nullptr) ? g_ctrl->project()->height() : 0; }
static auto api_project_get_fps() -> double { return (g_ctrl != nullptr) ? g_ctrl->project()->fps() : 0.0; }

static void api_scene_create(const char *name) {
    if (g_ctrl != nullptr) {
        g_ctrl->createScene(QString::fromUtf8(name));
    }
}
static void api_scene_switch(int id) {
    if (g_ctrl != nullptr) {
        g_ctrl->switchScene(id);
    }
}

static void api_command_begin_group(const char *text) {
    if ((g_ctrl != nullptr) && (g_ctrl->timeline() != nullptr)) {
        g_ctrl->timeline()->undoStack()->beginMacro(QString::fromUtf8(text));
    }
}
static void api_command_end_group() {
    if ((g_ctrl != nullptr) && (g_ctrl->timeline() != nullptr)) {
        g_ctrl->timeline()->undoStack()->endMacro();
    }
}
}

static HostApiTable g_hostApi = {.log = api_log,
                                 .transport_play = api_transport_play,
                                 .transport_pause = api_transport_pause,
                                 .transport_toggle = api_transport_toggle,
                                 .transport_seek = api_transport_seek,
                                 .transport_get_frame = api_transport_get_frame,
                                 .transport_is_playing = api_transport_is_playing,
                                 .clip_create = api_clip_create,
                                 .clip_delete = api_clip_delete,
                                 .clip_update = api_clip_update,
                                 .clip_select = api_clip_select,
                                 .project_get_width = api_project_get_width,
                                 .project_get_height = api_project_get_height,
                                 .project_get_fps = api_project_get_fps,
                                 .scene_create = api_scene_create,
                                 .scene_switch = api_scene_switch,
                                 .command_begin_group = api_command_begin_group,
                                 .command_end_group = api_command_end_group};

// ヘルパー
static auto _checkCtrl(lua_State *L) -> int {
    if (g_ctrl == nullptr) {
        lua_pushstring(L, "[AviQtlAPI] controller not ready");
        lua_error(L);
    }
    return 0;
}

// transport
static auto l_transport_play(lua_State *L) -> int {
    _checkCtrl(L);
    if (!g_ctrl->transport()->isPlaying()) {
        g_ctrl->transport()->togglePlay();
    }
    return 0;
}
static auto l_transport_pause(lua_State *L) -> int {
    _checkCtrl(L);
    if (g_ctrl->transport()->isPlaying()) {
        g_ctrl->transport()->togglePlay();
    }
    return 0;
}
static auto l_transport_toggle(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->transport()->togglePlay();
    return 0;
}
static auto l_transport_seek(lua_State *L) -> int {
    _checkCtrl(L);
    int frame = static_cast<int>(luaL_checkinteger(L, 1));
    g_ctrl->transport()->setCurrentFrame(frame);
    return 0;
}
static auto l_transport_get_frame(lua_State *L) -> int {
    _checkCtrl(L);
    lua_pushinteger(L, g_ctrl->transport()->currentFrame());
    return 1;
}
static auto l_transport_is_playing(lua_State *L) -> int {
    _checkCtrl(L);
    lua_pushboolean(L, static_cast<int>(g_ctrl->transport()->isPlaying()));
    return 1;
}

// clip
static auto l_clip_create(lua_State *L) -> int {
    _checkCtrl(L);
    // aviqtl_clip_create(type, startFrame, layer)
    const char *type = luaL_checkstring(L, 1);
    int startFrame = static_cast<int>(luaL_checkinteger(L, 2));
    int layer = static_cast<int>(luaL_checkinteger(L, 3));
    g_ctrl->createObject(QString::fromUtf8(type), startFrame, layer);
    return 0;
}
static auto l_clip_delete(lua_State *L) -> int {
    _checkCtrl(L);
    int clipId = static_cast<int>(luaL_checkinteger(L, 1));
    g_ctrl->deleteClip(clipId);
    return 0;
}
static auto l_clip_update(lua_State *L) -> int {
    _checkCtrl(L);
    // aviqtl_clip_update(clipId, layer, startFrame, duration)
    int id = static_cast<int>(luaL_checkinteger(L, 1));
    int layer = static_cast<int>(luaL_checkinteger(L, 2));
    int start = static_cast<int>(luaL_checkinteger(L, 3));
    int dur = static_cast<int>(luaL_checkinteger(L, 4));
    g_ctrl->updateClip(id, layer, start, dur);
    return 0;
}
static auto l_clip_select(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->selectClip(static_cast<int>(luaL_checkinteger(L, 1)));
    return 0;
}
static auto l_clip_split(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->splitClip(static_cast<int>(luaL_checkinteger(L, 1)), static_cast<int>(luaL_checkinteger(L, 2)));
    return 0;
}
static auto l_clip_copy(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->copyClip(static_cast<int>(luaL_checkinteger(L, 1)));
    return 0;
}
static auto l_clip_cut(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->cutClip(static_cast<int>(luaL_checkinteger(L, 1)));
    return 0;
}
static auto l_clip_paste(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->pasteClip(static_cast<int>(luaL_checkinteger(L, 1)), static_cast<int>(luaL_checkinteger(L, 2)));
    return 0;
}
static auto l_clip_list(lua_State *L) -> int {
    _checkCtrl(L);
    QVariantList clips = g_ctrl->clips();
    lua_newtable(L);
    for (int i = 0; i < clips.size(); i++) {
        QVariantMap m = clips.value(i).toMap();
        lua_newtable(L);
        auto push = [&](const char *k, const QVariant &v) -> void {
            lua_pushstring(L, k);
            if (v.typeId() == QMetaType::Int || v.typeId() == QMetaType::LongLong) {
                lua_pushinteger(L, v.toInt());
            } else if (v.typeId() == QMetaType::Double || v.typeId() == QMetaType::Float) {
                lua_pushnumber(L, v.toDouble());
            } else {
                lua_pushstring(L, v.toString().toUtf8().constData());
            }
            lua_settable(L, -3);
        };
        push("id", m.value(QStringLiteral("id")));
        push("type", m.value(QStringLiteral("type")));
        push("layer", m.value(QStringLiteral("layer")));
        push("startFrame", m.value(QStringLiteral("startFrame")));
        push("duration", m.value(QStringLiteral("durationFrames")));
        lua_rawseti(L, -2, i + 1);
    }
    return 1;
}

// effect
static auto l_effect_add(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->addEffect(static_cast<int>(luaL_checkinteger(L, 1)), QString::fromUtf8(luaL_checkstring(L, 2)));
    return 0;
}
static auto l_effect_remove(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->removeEffect(static_cast<int>(luaL_checkinteger(L, 1)), static_cast<int>(luaL_checkinteger(L, 2)));
    return 0;
}
static auto l_effect_set_param(lua_State *L) -> int {
    _checkCtrl(L);
    // aviqtl_effect_set_param(clipId, effectIndex, paramName, value)
    int clipId = static_cast<int>(luaL_checkinteger(L, 1));
    int effectIndex = static_cast<int>(luaL_checkinteger(L, 2));
    const char *key = luaL_checkstring(L, 3);
    QVariant val;
    if (lua_type(L, 4) == LUA_TNUMBER) {
        val = lua_tonumber(L, 4);
    } else if (lua_type(L, 4) == LUA_TBOOLEAN) {
        val = static_cast<bool>(lua_toboolean(L, 4));
    } else {
        val = QString::fromUtf8(lua_tostring(L, 4));
    }
    g_ctrl->updateClipEffectParam(clipId, effectIndex, QString::fromUtf8(key), val);
    return 0;
}

// project
static auto l_project_get_width(lua_State *L) -> int {
    _checkCtrl(L);
    lua_pushinteger(L, g_ctrl->project()->width());
    return 1;
}
static auto l_project_get_height(lua_State *L) -> int {
    _checkCtrl(L);
    lua_pushinteger(L, g_ctrl->project()->height());
    return 1;
}
static auto l_project_get_fps(lua_State *L) -> int {
    _checkCtrl(L);
    lua_pushnumber(L, g_ctrl->project()->fps());
    return 1;
}
static auto l_project_save(lua_State *L) -> int {
    _checkCtrl(L);
    bool ok = g_ctrl->saveProject(QString::fromUtf8(luaL_checkstring(L, 1)));
    lua_pushboolean(L, static_cast<int>(ok));
    return 1;
}
static auto l_project_load(lua_State *L) -> int {
    _checkCtrl(L);
    bool ok = g_ctrl->loadProject(QString::fromUtf8(luaL_checkstring(L, 1)));
    lua_pushboolean(L, static_cast<int>(ok));
    return 1;
}

// undo/redo
static auto l_undo(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->undo();
    return 0;
}
static auto l_redo(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->redo();
    return 0;
}

// scene
static auto l_scene_create(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->createScene(QString::fromUtf8(luaL_checkstring(L, 1)));
    return 0;
}
static auto l_scene_remove(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->removeScene(static_cast<int>(luaL_checkinteger(L, 1)));
    return 0;
}
static auto l_scene_switch(lua_State *L) -> int {
    _checkCtrl(L);
    g_ctrl->switchScene(static_cast<int>(luaL_checkinteger(L, 1)));
    return 0;
}

// command
static auto l_command_begin_group(lua_State *L) -> int {
    _checkCtrl(L);
    const char *text = luaL_checkstring(L, 1);
    if (g_ctrl->timeline() != nullptr) {
        g_ctrl->timeline()->undoStack()->beginMacro(QString::fromUtf8(text));
    }
    return 0;
}
static auto l_command_end_group(lua_State *L) -> int {
    _checkCtrl(L);
    if (g_ctrl->timeline() != nullptr) {
        g_ctrl->timeline()->undoStack()->endMacro();
    }
    return 0;
}

auto ModEngine::instance() -> ModEngine & {
    static ModEngine inst;
    return inst;
}

ModEngine::~ModEngine() {
    if (L != nullptr) {
        lua_close(L);
    }
}

void ModEngine::initialize(void *ecsPtr) {
    if (L != nullptr) {
        return;
    }
    L = luaL_newstate();
    luaL_openlibs(L); // 全標準ライブラリ（io, os, debug, ffi等）を解放

    // 名前を "AVIQTL_CORE_PTR" に統一して登録
    lua_pushlightuserdata(L, ecsPtr);
    lua_setglobal(L, "AVIQTL_CORE_PTR");
    qInfo() << "[ModEngine] LuaJIT initialized. Core pointer registered as AVIQTL_CORE_PTR";
}

void ModEngine::registerController(void *controller) {
    g_ctrl = static_cast<AviQtl::UI::TimelineController *>(controller);
    if (L != nullptr) {
        _registerAviQtlAPI();

        // Export Host API Table
        lua_pushlightuserdata(L, &g_hostApi);
        lua_setglobal(L, "AVIQTL_HOST_API");
    }
}

void ModEngine::_registerAviQtlAPI() {
    // transport
    lua_register(L, "aviqtl_transport_play", l_transport_play);
    lua_register(L, "aviqtl_transport_pause", l_transport_pause);
    lua_register(L, "aviqtl_transport_toggle", l_transport_toggle);
    lua_register(L, "aviqtl_transport_seek", l_transport_seek);
    lua_register(L, "aviqtl_transport_get_frame", l_transport_get_frame);
    lua_register(L, "aviqtl_transport_is_playing", l_transport_is_playing);
    // clip
    lua_register(L, "aviqtl_clip_create", l_clip_create);
    lua_register(L, "aviqtl_clip_delete", l_clip_delete);
    lua_register(L, "aviqtl_clip_update", l_clip_update);
    lua_register(L, "aviqtl_clip_select", l_clip_select);
    lua_register(L, "aviqtl_clip_split", l_clip_split);
    lua_register(L, "aviqtl_clip_copy", l_clip_copy);
    lua_register(L, "aviqtl_clip_cut", l_clip_cut);
    lua_register(L, "aviqtl_clip_paste", l_clip_paste);
    lua_register(L, "aviqtl_clip_list", l_clip_list);
    // effect
    lua_register(L, "aviqtl_effect_add", l_effect_add);
    lua_register(L, "aviqtl_effect_remove", l_effect_remove);
    lua_register(L, "aviqtl_effect_set_param", l_effect_set_param);
    // project
    lua_register(L, "aviqtl_project_width", l_project_get_width);
    lua_register(L, "aviqtl_project_height", l_project_get_height);
    lua_register(L, "aviqtl_project_fps", l_project_get_fps);
    lua_register(L, "aviqtl_project_save", l_project_save);
    lua_register(L, "aviqtl_project_load", l_project_load);
    // undo/redo
    lua_register(L, "aviqtl_undo", l_undo);
    lua_register(L, "aviqtl_redo", l_redo);
    // scene
    lua_register(L, "aviqtl_scene_create", l_scene_create);
    lua_register(L, "aviqtl_scene_remove", l_scene_remove);
    lua_register(L, "aviqtl_scene_switch", l_scene_switch);
    // command
    lua_register(L, "aviqtl_command_begin_group", l_command_begin_group);
    lua_register(L, "aviqtl_command_end_group", l_command_end_group);

    // aviqtl.xxx() 形式のテーブルAPIをLua側で構築
    const char *aviqtl_table = R"(
aviqtl = {
    transport = {
        play       = aviqtl_transport_play,
        pause      = aviqtl_transport_pause,
        toggle     = aviqtl_transport_toggle,
        seek       = aviqtl_transport_seek,
        get_frame  = aviqtl_transport_get_frame,
        is_playing = aviqtl_transport_is_playing,
    },
    clip = {
        create = aviqtl_clip_create,
        delete = aviqtl_clip_delete,
        update = aviqtl_clip_update,
        select = aviqtl_clip_select,
        split  = aviqtl_clip_split,
        copy   = aviqtl_clip_copy,
        cut    = aviqtl_clip_cut,
        paste  = aviqtl_clip_paste,
        list   = aviqtl_clip_list,
    },
    effect = {
        add       = aviqtl_effect_add,
        remove    = aviqtl_effect_remove,
        set_param = aviqtl_effect_set_param,
    },
    project = {
        width        = aviqtl_project_width,
        height       = aviqtl_project_height,
        fps          = aviqtl_project_fps,
        save         = aviqtl_project_save,
        load         = aviqtl_project_load,
    },
    scene = {
        create = aviqtl_scene_create,
        remove = aviqtl_scene_remove,
        switch = aviqtl_scene_switch,
    },
    command = {
        begin_group = aviqtl_command_begin_group,
        end_group = aviqtl_command_end_group,
    },
    undo = aviqtl_undo,
    redo = aviqtl_redo,
}
)";
    // Lua の delete/switch は予約語なので _G 経由でアクセスする場合のみ注意
    luaL_dostring(L, aviqtl_table);

    qInfo() << "[ModEngine] AviQtl Lua API registered.";
}

void ModEngine::loadPlugins() {
    QString pluginsPath = QCoreApplication::applicationDirPath() + QLatin1String("/plugins");
    QDir dir(pluginsPath);

    if (!dir.exists()) {
        dir.mkpath(QStringLiteral("."));
        return;
    }

    QStringList filters;
    filters << QStringLiteral("*.lua");
    QFileInfoList files = dir.entryInfoList(filters, QDir::Files, QDir::Name);

    for (const QFileInfo &fileInfo : files) {
        qInfo() << "[ModEngine] Loading MOD:" << fileInfo.fileName();
        if (luaL_dofile(L, fileInfo.absoluteFilePath().toUtf8().constData())) {
            qCritical() << "[ModEngine] Load Error:" << lua_tostring(L, -1);
            lua_pop(L, 1);
        }
    }
}

void ModEngine::onUpdate() {
    if (L == nullptr) {
        return;
    }
    lua_getglobal(L, "AviQtlUpdateHook");
    if (lua_isfunction(L, -1)) {
        if (lua_pcall(L, 0, 0, 0) != 0) {
            qCritical() << "[ModEngine] Hook Error:" << lua_tostring(L, -1);
            lua_pop(L, 1);
        }
    } else {
        lua_pop(L, 1);
    }
}

} // namespace AviQtl::Scripting