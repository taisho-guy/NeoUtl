#include "project_service.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::UI;

class TestProjectService : public QObject {
    Q_OBJECT

  private slots:
    void defaultValues() {
        ProjectService svc;
        // Defaults come from SettingsManager, which we already tested.
        // Just verify getters return consistent values.
        QVERIFY(svc.width() > 0);
        QVERIFY(svc.height() > 0);
        QVERIFY(svc.fps() > 0);
        QVERIFY(svc.sampleRate() > 0);
    }

    void setWidth() {
        ProjectService svc;
        int original = svc.width();
        QSignalSpy spy(&svc, &ProjectService::widthChanged);
        svc.setWidth(original + 10);
        QCOMPARE(svc.width(), original + 10);
        QCOMPARE(spy.count(), 1);
    }

    void setWidthNoChange() {
        ProjectService svc;
        int width = svc.width();
        QSignalSpy spy(&svc, &ProjectService::widthChanged);
        svc.setWidth(width);
        QCOMPARE(spy.count(), 0);
    }

    void setHeight() {
        ProjectService svc;
        int original = svc.height();
        QSignalSpy spy(&svc, &ProjectService::heightChanged);
        svc.setHeight(original + 10);
        QCOMPARE(svc.height(), original + 10);
        QCOMPARE(spy.count(), 1);
    }

    void setHeightNoChange() {
        ProjectService svc;
        int height = svc.height();
        QSignalSpy spy(&svc, &ProjectService::heightChanged);
        svc.setHeight(height);
        QCOMPARE(spy.count(), 0);
    }

    void setFps() {
        ProjectService svc;
        QSignalSpy spy(&svc, &ProjectService::fpsChanged);
        svc.setFps(30.0);
        QCOMPARE(svc.fps(), 30.0);
        QCOMPARE(spy.count(), 1);
    }

    void setFpsNoChange() {
        ProjectService svc;
        double fps = svc.fps();
        QSignalSpy spy(&svc, &ProjectService::fpsChanged);
        svc.setFps(fps);
        QCOMPARE(spy.count(), 0);
    }

    void setSampleRate() {
        ProjectService svc;
        QSignalSpy spy(&svc, &ProjectService::sampleRateChanged);
        svc.setSampleRate(44100);
        QCOMPARE(svc.sampleRate(), 44100);
        QCOMPARE(spy.count(), 1);
    }

    void setSampleRateNoChange() {
        ProjectService svc;
        int rate = svc.sampleRate();
        QSignalSpy spy(&svc, &ProjectService::sampleRateChanged);
        svc.setSampleRate(rate);
        QCOMPARE(spy.count(), 0);
    }
};

QTEST_MAIN(TestProjectService)
#include "test_project_service.moc"
