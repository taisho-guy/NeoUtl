#include "app_main.h"
#include "bridge_qt.h"
#include "workspace_qt.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QQuickItem>
#include <QtCore/QUrl>
#include <QtCore/QString>
#include <QtCore/QDebug>
#include <QtCore/QByteArray>
#include <cstdlib>

extern "C" void* launch_winit_wgpu_layer();

namespace {

void embedForeignWindow(QWindow *foreignWindow, QQuickWindow *hostWindow, QQuickItem *surfaceArea) {
    foreignWindow->setParent(hostWindow);

    auto syncGeometry = [foreignWindow, hostWindow, surfaceArea]() {
        const QPointF topLeft = surfaceArea->mapToItem(hostWindow->contentItem(), QPointF(0, 0));
        foreignWindow->setGeometry(
            static_cast<int>(topLeft.x()),
            static_cast<int>(topLeft.y()),
            static_cast<int>(surfaceArea->width()),
            static_cast<int>(surfaceArea->height()));
    };

    QObject::connect(surfaceArea, &QQuickItem::xChanged, foreignWindow, syncGeometry);
    QObject::connect(surfaceArea, &QQuickItem::yChanged, foreignWindow, syncGeometry);
    QObject::connect(surfaceArea, &QQuickItem::widthChanged, foreignWindow, syncGeometry);
    QObject::connect(surfaceArea, &QQuickItem::heightChanged, foreignWindow, syncGeometry);

    syncGeometry();
    foreignWindow->show();
}

} 
void run_qt_application(rust::Str qml_url) {
    static int argc = 1;
    static char app_name[] = "NeoQtl";
    static char *argv[] = { app_name, nullptr };

#ifdef Q_OS_LINUX
    qputenv("QT_QPA_PLATFORM", "xcb");
#endif

    QGuiApplication app(argc, argv);

    QQmlApplicationEngine engine;
    register_timeline_bridge(engine);
    register_workspace_bridge(engine);

    void* nativeWindowHandle = launch_winit_wgpu_layer();
    QWindow* foreignWindow = nullptr;

    if (nativeWindowHandle) {
        WId winitWindowId = reinterpret_cast<WId>(nativeWindowHandle);
        foreignWindow = QWindow::fromWinId(winitWindowId);
        if (!foreignWindow) {
            qWarning() << "[NeoQtl] QWindow::fromWinId returned null for handle:" << winitWindowId;
        }
    } else {
        qWarning() << "[NeoQtl] launch_winit_wgpu_layer returned null handle";
    }

    const QString url = QString::fromUtf8(qml_url.data(), static_cast<int>(qml_url.size()));
    engine.load(QUrl(url));

    if (engine.rootObjects().isEmpty()) {
        qCritical() << "[NeoQtl] QMLロード失敗、起動中止:" << url;
        if (foreignWindow) {
            delete foreignWindow;
        }
        std::exit(1);
    }

    if (foreignWindow) {
        auto *hostWindow = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        QQuickItem *surfaceArea = hostWindow
            ? hostWindow->findChild<QQuickItem *>("previewSurfaceArea")
            : nullptr;

        if (hostWindow && surfaceArea) {
            embedForeignWindow(foreignWindow, hostWindow, surfaceArea);
            qDebug() << "[NeoQtl] Winit window embedded into previewSurfaceArea";
        } else {
            qWarning() << "[NeoQtl] previewSurfaceArea未検出、埋め込み不可";
            delete foreignWindow;
            foreignWindow = nullptr;
        }
    }

    app.exec();

    if (foreignWindow) {
        delete foreignWindow;
    }
}
