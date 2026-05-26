#include "engine/plugin/audio_plugin_chain.hpp"
#include <QTest>
#include <vector>

using namespace AviQtl::Engine::Plugin;

class MockPlugin : public IAudioPlugin {
  public:
    explicit MockPlugin(QString id = QStringLiteral("mock")) : m_id(std::move(id)) {}

    bool load(const QString &, int) override { return true; }
    void prepare(double sr, int bs) override {
        m_prepareCalls++;
        m_lastSampleRate = sr;
        m_lastBlockSize = bs;
    }
    void process(float *, int frameCount) override {
        m_processCalls++;
        m_lastFrameCount = frameCount;
    }
    void release() override {}

    QString name() const override { return m_id; }
    QString format() const override { return QStringLiteral("Mock"); }
    int paramCount() const override { return 0; }
    QString paramName(int) const override { return QStringLiteral(""); }
    float getParam(int) const override { return 0.0f; }
    void setParam(int, float) override {}

    int prepareCalls() const { return m_prepareCalls; }
    int processCalls() const { return m_processCalls; }
    double lastSampleRate() const { return m_lastSampleRate; }
    int lastBlockSize() const { return m_lastBlockSize; }
    int lastFrameCount() const { return m_lastFrameCount; }

  private:
    QString m_id;
    int m_prepareCalls = 0;
    int m_processCalls = 0;
    double m_lastSampleRate = 0;
    int m_lastBlockSize = 0;
    int m_lastFrameCount = 0;
};

class TestAudioPluginChain : public QObject {
    Q_OBJECT

  private slots:
    void constructorReadsDefaults() {
        AudioPluginChain chain;
        // Just verify it does not crash; defaults come from SettingsManager which is
        // already tested elsewhere.
        QCOMPARE(chain.count(), 0);
    }

    void addAndCount() {
        AudioPluginChain chain;
        chain.add(std::make_unique<MockPlugin>());
        QCOMPARE(chain.count(), 1);
        chain.add(std::make_unique<MockPlugin>());
        QCOMPARE(chain.count(), 2);
    }

    void addCallsPrepare() {
        AudioPluginChain chain;
        auto mock = std::make_unique<MockPlugin>();
        MockPlugin *raw = mock.get();
        chain.add(std::move(mock));
        QCOMPARE(raw->prepareCalls(), 1);
    }

    void get() {
        AudioPluginChain chain;
        auto p1 = std::make_unique<MockPlugin>(QStringLiteral("A"));
        auto p2 = std::make_unique<MockPlugin>(QStringLiteral("B"));
        chain.add(std::move(p1));
        chain.add(std::move(p2));

        QCOMPARE(chain.count(), 2);
        QCOMPARE(chain.get(0)->name(), QStringLiteral("A"));
        QCOMPARE(chain.get(1)->name(), QStringLiteral("B"));
    }

    void getOutOfBounds() {
        AudioPluginChain chain;
        QVERIFY(chain.get(0) == nullptr);
        QVERIFY(chain.get(-1) == nullptr);
        chain.add(std::make_unique<MockPlugin>());
        QVERIFY(chain.get(1) == nullptr);
    }

    void remove() {
        AudioPluginChain chain;
        chain.add(std::make_unique<MockPlugin>(QStringLiteral("A")));
        chain.add(std::make_unique<MockPlugin>(QStringLiteral("B")));
        QCOMPARE(chain.count(), 2);

        chain.remove(0);
        QCOMPARE(chain.count(), 1);
        QCOMPARE(chain.get(0)->name(), QStringLiteral("B"));
    }

    void removeOutOfBounds() {
        AudioPluginChain chain;
        chain.add(std::make_unique<MockPlugin>());
        chain.remove(-1); // should not crash
        chain.remove(99); // should not crash
        QCOMPARE(chain.count(), 1);
    }

    void clear() {
        AudioPluginChain chain;
        chain.add(std::make_unique<MockPlugin>());
        chain.add(std::make_unique<MockPlugin>());
        QCOMPARE(chain.count(), 2);

        chain.clear();
        QCOMPARE(chain.count(), 0);
        QVERIFY(chain.get(0) == nullptr);
    }

    void preparePropagates() {
        AudioPluginChain chain;
        auto p1 = std::make_unique<MockPlugin>();
        auto p2 = std::make_unique<MockPlugin>();
        MockPlugin *raw1 = p1.get();
        MockPlugin *raw2 = p2.get();
        chain.add(std::move(p1));
        chain.add(std::move(p2));

        // add() already called prepare with defaults; reset counters by re-preparing
        chain.prepare(44100.0, 512);
        QCOMPARE(raw1->prepareCalls(), 2); // initial + re-prepare
        QCOMPARE(raw2->prepareCalls(), 2);
        QCOMPARE(raw1->lastSampleRate(), 44100.0);
        QCOMPARE(raw1->lastBlockSize(), 512);
    }

    void processIterates() {
        AudioPluginChain chain;
        auto p1 = std::make_unique<MockPlugin>();
        auto p2 = std::make_unique<MockPlugin>();
        MockPlugin *raw1 = p1.get();
        MockPlugin *raw2 = p2.get();
        chain.add(std::move(p1));
        chain.add(std::move(p2));

        std::vector<float> buf(4, 0.0f);
        chain.process(buf.data(), static_cast<int>(buf.size()) / 2); // 2 frames

        QCOMPARE(raw1->processCalls(), 1);
        QCOMPARE(raw2->processCalls(), 1);
        QCOMPARE(raw1->lastFrameCount(), 2);
        QCOMPARE(raw2->lastFrameCount(), 2);
    }

    void processOnEmptyChain() {
        AudioPluginChain chain;
        std::vector<float> buf(4, 0.0f);
        // Should not crash; does nothing.
        chain.process(buf.data(), 2);
        QCOMPARE(buf[0], 0.0f);
    }

    void prepareEmptyChain() {
        AudioPluginChain chain;
        // Should not crash on empty chain
        chain.prepare(96000.0, 2048);
        QCOMPARE(chain.count(), 0);
    }

    void addAfterPreparePropagatesSettings() {
        AudioPluginChain chain;
        chain.prepare(44100.0, 512);

        auto mock = std::make_unique<MockPlugin>();
        MockPlugin *raw = mock.get();
        chain.add(std::move(mock));

        QCOMPARE(raw->prepareCalls(), 1);
        QCOMPARE(raw->lastSampleRate(), 44100.0);
        QCOMPARE(raw->lastBlockSize(), 512);
    }

    void removeMiddle() {
        AudioPluginChain chain;
        chain.add(std::make_unique<MockPlugin>(QStringLiteral("A")));
        chain.add(std::make_unique<MockPlugin>(QStringLiteral("B")));
        chain.add(std::make_unique<MockPlugin>(QStringLiteral("C")));
        QCOMPARE(chain.count(), 3);

        chain.remove(1);
        QCOMPARE(chain.count(), 2);
        QCOMPARE(chain.get(0)->name(), QStringLiteral("A"));
        QCOMPARE(chain.get(1)->name(), QStringLiteral("C"));
    }

    void processChainOrder() {
        // Use a mock plugin that adds 1.0 to every sample, then another that multiplies by 2.0
        class AddOnePlugin : public MockPlugin {
          public:
            AddOnePlugin() : MockPlugin(QStringLiteral("add1")) {}
            void process(float *buf, int count) override {
                for (int i = 0; i < count; ++i)
                    buf[i] += 1.0f;
            }
        };
        class MulTwoPlugin : public MockPlugin {
          public:
            MulTwoPlugin() : MockPlugin(QStringLiteral("mul2")) {}
            void process(float *buf, int count) override {
                for (int i = 0; i < count; ++i)
                    buf[i] *= 2.0f;
            }
        };

        AudioPluginChain chain;
        chain.add(std::make_unique<AddOnePlugin>());
        chain.add(std::make_unique<MulTwoPlugin>());

        // buf starts at [2, 4, 6, 8] (4 samples = 2 stereo frames)
        // After add1: [3, 5, 7, 9]
        // After mul2: [6, 10, 14, 18]
        std::vector<float> buf = {2.0f, 4.0f, 6.0f, 8.0f};
        chain.process(buf.data(), 4); // 4 mono samples or 2 stereo frames

        QCOMPARE(buf[0], 6.0f);
        QCOMPARE(buf[1], 10.0f);
        QCOMPARE(buf[2], 14.0f);
        QCOMPARE(buf[3], 18.0f);
    }
};

QTEST_MAIN(TestAudioPluginChain)
#include "test_audio_plugin_chain.moc"
