#include <QApplication>
#include <QDir>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QSplashScreen>
#include <QTimer>
#include <QTranslator>

// Core Headers
#include "compute_effect.hpp"
#include "effect_registry.hpp"
#include "package_manager.hpp"
#include "settings_manager.hpp"
#include "theme_controller.hpp"
#include "version.hpp"
#include "video_encoder.hpp"
#include "video_frame_provider.hpp"
#include "video_frame_store.hpp"

// UI Headers
#include "../../ui/include/bridge/core_bridge.hpp"
#include "../../ui/include/timeline_controller.hpp"
#include "../../ui/include/window_manager.hpp"
#include "../../ui/include/workspace.hpp"

// Engine / Scripting Headers
#include "../../engine/plugin/audio_plugin_manager.hpp"
#include "../../scripting/mod_engine.hpp"

extern "C" {
#include <libavutil/log.h>
}

using namespace AviQtl;

static void aviqtl_ffmpeg_log_callback(void *ptr, int level, const char *fmt, va_list vl) {
    char line[1024];
    va_list vl_copy;
    va_copy(vl_copy, vl);
    vsnprintf(line, sizeof(line), fmt, vl_copy);
    va_end(vl_copy);
    if (strstr(line, "Late SEI is not implemented") != nullptr)
        return;
    av_log_default_callback(ptr, level, fmt, vl);
}

void setupQmlEngine(QQmlApplicationEngine &engine) {
    QQuickStyle::setFallbackStyle(QStringLiteral("Fusion"));

    // 型登録
    qmlRegisterType<Core::VideoEncoder>("AviQtl.Core", 1, 0, "VideoEncoder");
    qmlRegisterType<UI::Effects::ComputeEffect>("AviQtl.Compute", 1, 0, "ComputeEffect");
    qmlRegisterUncreatableType<UI::TimelineController>("AviQtl.UI", 1, 0, "TimelineController", "Managed by C++");
    qmlRegisterSingletonInstance<UI::CoreBridge>("AviQtl.UI", 1, 0, "CoreBridge", &UI::CoreBridge::instance());

    auto *ctx = engine.rootContext();
    ctx->setContextProperty(QStringLiteral("SettingsManager"), &Core::SettingsManager::instance());
    ctx->setContextProperty(QStringLiteral("AviQtlVersion"), QString::fromUtf8(VERSION_STRING));
    ctx->setContextProperty(QStringLiteral("PackageManager"), &Core::PackageManager::instance());
    ctx->setContextProperty(QStringLiteral("WindowManager"), static_cast<QObject *>(&UI::WindowManager::instance()));
}

auto main(int argc, char *argv[]) -> int {
    QApplication app(argc, argv);
    app.setQuitOnLastWindowClosed(false);
    QApplication::setApplicationName(QStringLiteral("AviQtl"));
    av_log_set_callback(aviqtl_ffmpeg_log_callback);
    QApplication::setWindowIcon(QIcon(QStringLiteral(":/assets/icon.svg")));

    // macOS .app bundle では Resources が ../Resources にある
    const QString appDir = QApplication::applicationDirPath();
    QString resourceDir = QDir(appDir + QStringLiteral("/../Resources")).canonicalPath();
    if (resourceDir.isEmpty()) {
        resourceDir = appDir;
    }

    // 翻訳
    QTranslator translator;
    if (translator.load(QLocale::system(), QStringLiteral("AviQtl"), QStringLiteral("_"), resourceDir + QStringLiteral("/i18n"))) {
        app.installTranslator(&translator);
    }

    // 設定・テーマ初期化（SettingsManagerはコンストラクタで設定をロードし、ThemeControllerがテーマを適用する）
    auto &settings = Core::SettingsManager::instance();
    Core::ThemeController::instance();

    // スプラッシュ
    int splashSize = settings.value(QStringLiteral("splashSize"), 128).toInt();
    QSplashScreen splash(QIcon(QStringLiteral(":/assets/splash.svg")).pixmap(splashSize, splashSize));
    splash.show();

    QQmlApplicationEngine engine;
    setupQmlEngine(engine);

    // サービス初期化
    auto *videoFrameStore = new Core::VideoFrameStore(&app);
    engine.addImageProvider(QStringLiteral("videoFrame"), new Core::VideoFrameProvider(videoFrameStore));
    engine.rootContext()->setContextProperty(QStringLiteral("videoFrameStore"), videoFrameStore);

    auto *workspace = new UI::Workspace(&app);
    workspace->setVideoFrameStore(videoFrameStore);
    engine.rootContext()->setContextProperty(QStringLiteral("Workspace"), workspace);

    auto &modEngine = Scripting::ModEngine::instance();
    QObject::connect(workspace, &UI::Workspace::currentTimelineChanged, [&]() {
        if (workspace->currentTimeline()) {
            modEngine.registerController(workspace->currentTimeline());
            UI::WindowManager::instance().spawnInitialWindows(&engine);
        }
    });

    QTimer luaTimer;
    QObject::connect(&luaTimer, &QTimer::timeout, [&]() { modEngine.onUpdate(); });
    luaTimer.start(settings.value(QStringLiteral("luaHookIntervalMs"), 16).toInt());

    // プラグインロード
    QTimer::singleShot(10, [&]() {
        modEngine.initialize(nullptr);
        modEngine.loadPlugins();

        auto &sm = Core::SettingsManager::instance();

        auto loadRegistry = [&](const QString &key) {
            if (sm.value(QStringLiteral("pluginEnable") + key, true).toBool()) {
                const QStringList paths = sm.value(QStringLiteral("pluginPaths") + key).toStringList();
                const QDir appDir(QCoreApplication::applicationDirPath());
                // macOS bundle 用の Resources ディレクトリも考慮
                QString resPath = QDir(appDir.absolutePath() + QStringLiteral("/../Resources")).canonicalPath();
                const QDir resDir(resPath.isEmpty() ? appDir.absolutePath() : resPath);

                for (const QString &path : paths) {
                    if (!path.isEmpty()) {
                        // 相対パスの場合はアプリ（またはResources）ディレクトリを起点として解決
                        QString absolutePath = QDir::isAbsolutePath(path) ? path : resDir.absoluteFilePath(path);
                        if (QFile::exists(absolutePath)) {
                            Core::EffectRegistry::instance().loadEffectsFromDirectory(absolutePath);
                        }
                    }
                }
            }
        };

        loadRegistry(QStringLiteral("Effects"));
        loadRegistry(QStringLiteral("Objects"));

        // シグナル発行がバックグラウンドスレッドからのため、
        // 第三引数に &app (メインスレッド所属) を渡してメインスレッドで実行されるようにする
        QObject::connect(&Engine::Plugin::AudioPluginManager::instance(), &Engine::Plugin::AudioPluginManager::pluginsReady, &app, [&]() {
            UI::WindowManager::instance().showLauncher(&engine);
            splash.finish(nullptr);
        });
        Engine::Plugin::AudioPluginManager::instance().initialize();
    });

    return app.exec();
}
