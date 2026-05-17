#include "window_manager.hpp"
#include "workspace.hpp"
#include <QCoreApplication>
#include <QDebug>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtQml>

namespace AviQtl::UI {

WindowManager::WindowManager(QObject *parent) : QObject(parent) {}

auto WindowManager::instance() -> WindowManager & {
    static WindowManager inst(nullptr);
    return inst;
}

void WindowManager::spawnInitialWindows(QQmlEngine *engine) {
    m_engine = engine;

    // ランチャーを経由せず直接メインウィンドウ群を生成する
    spawnWindow(engine, QStringLiteral("main"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/MainWindow.qml"), tr("AviQtl メインプレビュー"), 640, 480, 100, 100, true);
    spawnWindow(engine, QStringLiteral("timeline"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/TimelineWindow.qml"), tr("タイムライン"), 1280, 300, 100, 600, true);
    spawnWindow(engine, QStringLiteral("projectSettings"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/ProjectSettingsWindow.qml"), tr("プロジェクト設定"), 450, 250, 800, 100, false);
    spawnWindow(engine, QStringLiteral("objectSettings"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/SettingDialog.qml"), tr("オブジェクト設定"), 400, 600, 800, 420, false);
    spawnWindow(engine, QStringLiteral("systemSettings"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/SystemSettingsWindow.qml"), tr("システム設定"), 600, 500, 200, 200, false);
    spawnWindow(engine, QStringLiteral("about"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/AboutWindow.qml"), tr("AviQtlについて"), 400, 250, 400, 300, false);
    spawnWindow(engine, QStringLiteral("sceneSettings"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/SceneSettingsWindow.qml"), tr("シーン設定"), 450, 300, 300, 200, false);
    spawnWindow(engine, QStringLiteral("export"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/ExportDialog.qml"), tr("メディアの書き出し"), 620, 580, 240, 160, false);
    spawnWindow(engine, QStringLiteral("easingConfig"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/common/EasingConfigWindow.qml"), tr("補間設定"), 820, 540, 420, 180, false);
    spawnWindow(engine, QStringLiteral("packageManager"), QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/PackageManagerWindow.qml"), tr("パッケージマネージャー"), 600, 400, 500, 300, false);

    // タブが 0 の状態で起動しているので、ランチャーを即座に表示する
    showLauncher();
}

void WindowManager::showLauncher(QQmlEngine *engine) {
    if (engine) {
        m_engine = engine;
    }

    if (m_engine == nullptr) {
        qWarning() << "showLauncher: QMLエンジンが未初期化です";
        return;
    }

    // 既存のランチャーウィンドウがあれば前面に出すだけ
    QPointer<QQuickWindow> existing = m_windows.value(QStringLiteral("launcher"));
    if (existing) {
        existing->show();
        existing->raise();
        existing->requestActivate();
        return;
    }

    QQmlComponent component(m_engine, QUrl(QStringLiteral("qrc:/qt/qml/AviQtl/ui/qml/ProjectLauncherWindow.qml")));
    if (component.status() != QQmlComponent::Ready) {
        qWarning() << "ProjectLauncherWindow コンポーネントエラー:" << component.errorString();
        return;
    }
    QObject *obj = component.create();
    auto *launcher = qobject_cast<QQuickWindow *>(obj);
    if (launcher != nullptr) {
        registerWindow(QStringLiteral("launcher"), launcher);
        launcher->show();
    } else {
        if (obj) {
            qWarning() << "ProjectLauncherWindow は QQuickWindow ではありません。実際の型:" << obj->metaObject()->className();
            delete obj;
        } else {
            qWarning() << "ProjectLauncherWindow の生成に失敗しました";
        }
    }
}

void WindowManager::spawnWindow(QQmlEngine *engine, const QString &id, const QString &urlStr, const QString &title, int w, int h, int x, int y, bool visible) { // NOLINT(bugprone-easily-swappable-parameters)
    if (engine == nullptr) {
        qWarning() << "WindowManager: QMLエンジンがnullです！";
        return;
    }

    QQmlComponent comp(engine, QUrl(urlStr));
    if (comp.status() != QQmlComponent::Ready) {
        qWarning() << "QMLエラー (" << title << "):" << comp.errorString();
        return;
    }

    QObject *obj = comp.create();
    if (auto *win = qobject_cast<QQuickWindow *>(obj)) {
        win->setTitle(title);
        win->resize(w, h);
        win->setX(x);
        win->setY(y);
        registerWindow(id, win);
        if (visible) {
            win->show();
        } else {
            win->hide();
        }
    } else {
        // QQuickWindowではなかった場合
        auto *window = new QQuickWindow();
        window->setTitle(title);
        window->resize(w, h);
        window->setX(x);
        window->setY(y);

        auto *item = qobject_cast<QQuickItem *>(obj);
        if (item != nullptr) {
            item->setParentItem(window->contentItem());
        }
        registerWindow(id, window);
        if (visible) {
            window->show();
        } else {
            window->hide();
        }
    }
}

void WindowManager::registerWindow(const QString &id, QQuickWindow *win) {
    m_windows.insert(id, win);

    // hide/showした場合の同期
    connect(win, &QQuickWindow::visibleChanged, this, [this, id]() -> void { emitVisibilityChanged(id); });
    connect(win, &QObject::destroyed, this, [this, id]() -> void {
        m_windows.remove(id);
        emitVisibilityChanged(id);
    });

    // メインが閉じられたら全終了
    if (id == QStringLiteral("main")) {
        connect(win, &QQuickWindow::closing, this, [this](QQuickCloseEvent *e) -> void {
            Q_UNUSED(e);
            requestQuit();
        });
    }

    emitVisibilityChanged(id);
}

void WindowManager::emitVisibilityChanged(const QString &id) {
    if (id == QStringLiteral("timeline")) {
        emit timelineVisibleChanged();
    }
    if (id == QStringLiteral("projectSettings")) {
        emit projectSettingsVisibleChanged();
    }
    if (id == QStringLiteral("objectSettings")) {
        emit objectSettingsVisibleChanged();
    }
    if (id == QStringLiteral("systemSettings")) {
        emit systemSettingsVisibleChanged();
    }
}

auto WindowManager::isVisible(const QString &id) const -> bool {
    QPointer<QQuickWindow> w = m_windows.value(id);
    return w ? w->isVisible() : false;
}
void WindowManager::setVisible(const QString &id, bool visible) {
    QPointer<QQuickWindow> w = m_windows.value(id);
    if (!w) {
        return;
    }
    if (visible) {
        w->show();
    } else {
        w->hide();
    }
    if (visible) {
        w->raise();
        w->requestActivate();
    }
}
void WindowManager::toggleVisible(const QString &id) { setVisible(id, !isVisible(id)); }
void WindowManager::raiseWindow(const QString &id) {
    QPointer<QQuickWindow> w = m_windows.value(id);
    if (!w) {
        return;
    }
    w->show();
    w->raise();
    w->requestActivate();
}

auto WindowManager::getWindow(const QString &id) const -> QObject * { return m_windows.value(id); }

void WindowManager::requestQuit() {
    // 全Windowを閉じる
    for (auto it = m_windows.begin(); it != m_windows.end(); ++it) {
        if (it.value()) {
            it.value()->close();
        }
    }
    QCoreApplication::quit();
}

auto WindowManager::timelineVisible() const -> bool { return isVisible(QStringLiteral("timeline")); }
void WindowManager::setTimelineVisible(bool v) { setVisible(QStringLiteral("timeline"), v); }
auto WindowManager::projectSettingsVisible() const -> bool { return isVisible(QStringLiteral("projectSettings")); }
void WindowManager::setProjectSettingsVisible(bool v) { setVisible(QStringLiteral("projectSettings"), v); }
auto WindowManager::objectSettingsVisible() const -> bool { return isVisible(QStringLiteral("objectSettings")); }
void WindowManager::setObjectSettingsVisible(bool v) { setVisible(QStringLiteral("objectSettings"), v); }
auto WindowManager::systemSettingsVisible() const -> bool { return isVisible(QStringLiteral("systemSettings")); }
void WindowManager::setSystemSettingsVisible(bool v) { setVisible(QStringLiteral("systemSettings"), v); }
} // namespace AviQtl::UI
