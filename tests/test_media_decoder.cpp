#include "media_decoder.hpp"
#include <QSignalSpy>
#include <QTest>
#include <QUrl>

using namespace AviQtl::Core;

// ─── Concrete subclass for testing ───
class MockMediaDecoder : public MediaDecoder {
  public:
    explicit MockMediaDecoder(int clipId, QUrl source, QObject *parent = nullptr) : MediaDecoder(clipId, std::move(source), parent) {}

    void seek(qint64) override {}
    void setPlaying(bool) override {}
    void startDecoding() override { emit ready(); }
};

class TestMediaDecoder : public QObject {
    Q_OBJECT

  private slots:
    void scheduleStartUsesQueuedConnection() {
        MockMediaDecoder decoder(1, QUrl(QStringLiteral("test.mp4")));
        QSignalSpy spy(&decoder, &MockMediaDecoder::ready);
        QCOMPARE(spy.count(), 0);

        decoder.scheduleStart();

        // QueuedConnection means the signal does NOT fire synchronously
        QCOMPARE(spy.count(), 0);

        // Allow the Qt event loop to process the queued call
        QTRY_COMPARE(spy.count(), 1);
    }

    void constructorSetsClipId() {
        MockMediaDecoder decoder(42, QUrl(QStringLiteral("sample.avi")));
        QCOMPARE(decoder.clipId(), 42);
        QCOMPARE(decoder.source(), QUrl(QStringLiteral("sample.avi")));
    }

    void isReadyDefaultFalse() {
        MockMediaDecoder decoder(1, QUrl(QStringLiteral("x.mp4")));
        QVERIFY(!decoder.isReady());
    }

    void getSamplesDefaultEmpty() {
        MockMediaDecoder decoder(1, QUrl(QStringLiteral("x.mp4")));
        std::vector<float> samples = decoder.getSamples(0.0, 10);
        QVERIFY(samples.empty());
    }
};

QTEST_MAIN(TestMediaDecoder)
#include "test_media_decoder.moc"
