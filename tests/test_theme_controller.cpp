#include "theme_controller.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::Core;

class TestThemeController : public QObject {
    Q_OBJECT

  private slots:
    void defaultThemeNotEmpty() {
        QString t = ThemeController::instance().theme();
        QVERIFY(!t.isEmpty()); // "Dark" (default settings) or system
    }

    void setThemeEmitsSignal() {
        ThemeController &ctrl = ThemeController::instance();
        QString original = ctrl.theme();
        QString target = (original == QStringLiteral("Light")) ? QStringLiteral("Dark") : QStringLiteral("Light");

        QSignalSpy spy(&ctrl, &ThemeController::themeChanged);
        ctrl.setTheme(target);
        QCOMPARE(spy.count(), 1);

        // Restore original to avoid side effects for other tests
        ctrl.setTheme(original);
    }

    void setSameThemeNoSignal() {
        ThemeController &ctrl = ThemeController::instance();
        QString current = ctrl.theme();

        QSignalSpy spy(&ctrl, &ThemeController::themeChanged);
        ctrl.setTheme(current);
        QCOMPARE(spy.count(), 0);
    }
};

QTEST_MAIN(TestThemeController)
#include "test_theme_controller.moc"
