#include "settings_manager.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::Core;

class TestSettingsManager : public QObject {
    Q_OBJECT

  private slots:
    void defaultValue() {
        QVariant theme = SettingsManager::instance().value(QStringLiteral("theme"));
        QCOMPARE(theme.toString(), QStringLiteral("Dark"));
    }

    void defaultMaxImageSize() {
        QVariant val = SettingsManager::instance().value(QStringLiteral("maxImageSize"));
        QCOMPARE(val.toInt(), 8192);
    }

    void setAndGetValue() {
        // Use underscore prefix to avoid disk save side-effect
        SettingsManager::instance().setValue(QStringLiteral("_test.integer"), 42);
        QCOMPARE(SettingsManager::instance().value(QStringLiteral("_test.integer")).toInt(), 42);
    }

    void valueWithDefault() {
        QVariant val = SettingsManager::instance().value(QStringLiteral("_test.nonexistent"), QStringLiteral("fallback"));
        QCOMPARE(val.toString(), QStringLiteral("fallback"));
    }

    void unchangedValueNoSignal() {
        QSignalSpy spy(&SettingsManager::instance(), &SettingsManager::settingsChanged);
        SettingsManager::instance().setValue(QStringLiteral("theme"), QStringLiteral("Dark")); // same as default
        QCOMPARE(spy.count(), 0);
    }

    void changedValueEmitsSignal() {
        QSignalSpy spy(&SettingsManager::instance(), &SettingsManager::settingsChanged);
        SettingsManager::instance().setValue(QStringLiteral("_test.signal"), QStringLiteral("A"));
        QCOMPARE(spy.count(), 1);

        SettingsManager::instance().setValue(QStringLiteral("_test.signal"), QStringLiteral("B"));
        QCOMPARE(spy.count(), 2);
    }

    void shortcutsDefaultExists() {
        QVariantMap map = SettingsManager::instance().shortcuts();
        QVERIFY(map.contains(QStringLiteral("project.new")));
        QVERIFY(map.contains(QStringLiteral("edit.undo")));
        QVERIFY(map.contains(QStringLiteral("transport.playPause")));
        QCOMPARE(map.value(QStringLiteral("project.new")).toString(), QStringLiteral("Ctrl+N"));
    }

    void shortcutLookup() {
        QString val = SettingsManager::instance().shortcut(QStringLiteral("project.save"), QStringLiteral("fallback"));
        QCOMPARE(val, QStringLiteral("Ctrl+S"));
    }

    void shortcutFallback() {
        QString val = SettingsManager::instance().shortcut(QStringLiteral("_nonexistent.action"), QStringLiteral("None"));
        QCOMPARE(val, QStringLiteral("None"));
    }
};

QTEST_MAIN(TestSettingsManager)
#include "test_settings_manager.moc"
