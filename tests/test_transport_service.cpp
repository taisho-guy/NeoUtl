#include "transport_service.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::UI;

class TestTransportService : public QObject {
    Q_OBJECT

  private slots:
    void initialState() {
        TransportService svc;
        QCOMPARE(svc.currentFrame(), 0);
        QCOMPARE(svc.isPlaying(), false);
        QCOMPARE(svc.playbackSpeed(), 1.0);
        QCOMPARE(svc.fps(), 60.0);
        QCOMPARE(svc.totalFrames(), 0);
    }

    void setCurrentFrame() {
        TransportService svc;
        QSignalSpy spy(&svc, &TransportService::currentFrameChanged);
        svc.setCurrentFrame(42);
        QCOMPARE(svc.currentFrame(), 42);
        QCOMPARE(spy.count(), 1);
    }

    void setCurrentFrameNoChange() {
        TransportService svc;
        svc.setCurrentFrame(5);
        QSignalSpy spy(&svc, &TransportService::currentFrameChanged);
        svc.setCurrentFrame(5);
        QCOMPARE(spy.count(), 0);
    }

    void togglePlay() {
        TransportService svc;
        QSignalSpy spy(&svc, &TransportService::isPlayingChanged);
        QVERIFY(!svc.isPlaying());

        svc.togglePlay();
        QVERIFY(svc.isPlaying());
        QCOMPARE(spy.count(), 1);

        svc.togglePlay();
        QVERIFY(!svc.isPlaying());
        QCOMPARE(spy.count(), 2);
    }

    void playPause() {
        TransportService svc;
        QVERIFY(!svc.isPlaying());
        svc.play();
        QVERIFY(svc.isPlaying());
        svc.pause();
        QVERIFY(!svc.isPlaying());
        // play() on already playing should be no-op
        QSignalSpy spy(&svc, &TransportService::isPlayingChanged);
        svc.play();
        QVERIFY(svc.isPlaying());
        QCOMPARE(spy.count(), 1); // only one emission for play()
        svc.play();
        QCOMPARE(spy.count(), 1); // no duplicate
    }

    void seekResetsOrigin() {
        TransportService svc;
        svc.setCurrentFrame_seek(100);
        QCOMPARE(svc.currentFrame(), 100);
        // Internal state reset, but we can verify by checking it doesn't crash
    }

    void scrub() {
        TransportService svc;
        QVERIFY(!svc.isPlaying());
        svc.play();
        QVERIFY(svc.isPlaying());

        svc.beginScrub();
        QVERIFY(!svc.isPlaying()); // paused during scrub

        svc.scrubTo(50);
        QCOMPARE(svc.currentFrame(), 50);

        svc.scrubTo(50); // same frame, no-op
        QCOMPARE(svc.currentFrame(), 50);

        svc.endScrub();
        // should not resume because we started playing before scrub
        // ... wait Qt::QueuedConnection to process
        QTRY_COMPARE(svc.isPlaying(), true);
    }

    void scrubWhenNotPlaying() {
        TransportService svc;
        QVERIFY(!svc.isPlaying());
        svc.beginScrub();
        QVERIFY(!svc.isPlaying());
        svc.scrubTo(10);
        QCOMPARE(svc.currentFrame(), 10);
        svc.endScrub();
        QVERIFY(!svc.isPlaying());
    }

    void setPlaybackSpeed() {
        TransportService svc;
        QSignalSpy spy(&svc, &TransportService::playbackSpeedChanged);
        svc.setPlaybackSpeed(2.0);
        QCOMPARE(svc.playbackSpeed(), 2.0);
        QCOMPARE(spy.count(), 1);

        // Changing while playing should NOT emit (guard in code)
        svc.togglePlay();
        QVERIFY(svc.isPlaying());
        QSignalSpy spyPlaying(&svc, &TransportService::playbackSpeedChanged);
        svc.setPlaybackSpeed(0.5);
        QCOMPARE(spyPlaying.count(), 0);
    }

    void setFps() {
        TransportService svc;
        QSignalSpy spy(&svc, &TransportService::fpsChanged);
        svc.setFps(30.0);
        QCOMPARE(svc.fps(), 30.0);
        QCOMPARE(spy.count(), 1);

        svc.setFps(30.0);
        QCOMPARE(spy.count(), 1); // no duplicate
    }

    void setTotalFrames() {
        TransportService svc;
        QSignalSpy spy(&svc, &TransportService::totalFramesChanged);
        svc.setTotalFrames(300);
        QCOMPARE(svc.totalFrames(), 300);
        QCOMPARE(spy.count(), 1);

        svc.setTotalFrames(300);
        QCOMPARE(spy.count(), 1); // no duplicate
    }
};

QTEST_MAIN(TestTransportService)
#include "test_transport_service.moc"
