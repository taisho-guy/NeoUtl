#include "effect_model.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::UI;

class TestEffectModel : public QObject {
    Q_OBJECT

  private slots:
    void constructorInitializesDefaults() {
        EffectModel m(QStringLiteral("test.id"), QStringLiteral("Test"), QStringLiteral("effect"), {QStringLiteral("VFX")}, {{"opacity", QVariant(100)}, {"pos.x", QVariant(0)}});
        QCOMPARE(m.id(), QStringLiteral("test.id"));
        QCOMPARE(m.name(), QStringLiteral("Test"));
        QCOMPARE(m.kind(), QStringLiteral("effect"));
        QCOMPARE(m.categories().size(), 1);
        QVERIFY(m.isEnabled());
        QVERIFY(m.keyframeTracks().contains(QStringLiteral("opacity")));
        QVERIFY(m.keyframeTracks().contains(QStringLiteral("pos.x")));
        QCOMPARE(m.params().value(QStringLiteral("opacity")).toInt(), 100);
    }

    void cloneCopiesFields() {
        EffectModel original(QStringLiteral("id"), QStringLiteral("Name"), QStringLiteral("object"), {QStringLiteral("3D")}, {{"scale", QVariant(1.5)}});
        original.setEnabled(false);
        auto copy = std::unique_ptr<EffectModel>(original.clone());
        QCOMPARE(copy->id(), QStringLiteral("id"));
        QCOMPARE(copy->isEnabled(), false);
        QCOMPARE(copy->params().value(QStringLiteral("scale")).toDouble(), 1.5);
    }

    void setEnabledSignal() {
        EffectModel m(QStringLiteral("x"), QStringLiteral("Y"), QStringLiteral("effect"), QStringList());
        QSignalSpy spy(&m, &EffectModel::enabledChanged);
        m.setEnabled(true); // no change
        QCOMPARE(spy.count(), 0);
        m.setEnabled(false);
        QCOMPARE(spy.count(), 1);
    }

    void setParamUpdatesTrackStartValue() {
        EffectModel m(QStringLiteral("x"), QStringLiteral("Y"), QStringLiteral("effect"), QStringList(), {{"opacity", QVariant(0)}});
        QSignalSpy spy(&m, &EffectModel::paramsChanged);
        QSignalSpy kfSpy(&m, &EffectModel::keyframeTracksChanged);

        m.setParam(QStringLiteral("opacity"), QVariant(255));
        QCOMPARE(m.params().value(QStringLiteral("opacity")).toInt(), 255);
        QCOMPARE(spy.count(), 1);

        // start keyframe should also have been updated
        QVERIFY(m.keyframeTracks().contains(QStringLiteral("opacity")));
    }

    void evaluatedParamNoKeyframeReturnsFallback() {
        EffectModel m(QStringLiteral("x"), QStringLiteral("Y"), QStringLiteral("effect"), QStringList(), {{"volume", QVariant(0.8)}});
        QVariant val = m.evaluatedParam(QStringLiteral("volume"), 0);
        QCOMPARE(val.toDouble(), 0.8);
        val = m.evaluatedParam(QStringLiteral("volume"), 100);
        QCOMPARE(val.toDouble(), 0.8);
    }

    void availableEasings() {
        EffectModel m(QStringLiteral("x"), QStringLiteral("Y"), QStringLiteral("effect"), QStringList());
        QStringList easings = m.availableEasings();
        QVERIFY(!easings.isEmpty());
        QVERIFY(easings.contains(QStringLiteral("linear")));
        QVERIFY(easings.contains(QStringLiteral("ease_out_bounce")));
        QVERIFY(easings.contains(QStringLiteral("custom")));
        QVERIFY(easings.contains(QStringLiteral("random")));
    }

    void isEndpointFrame() {
        EffectModel m(QStringLiteral("x"), QStringLiteral("Y"), QStringLiteral("effect"), QStringList(), {{"pos", QVariant(0)}});
        QVERIFY(m.isEndpointFrame(QStringLiteral("pos"), 0));
        QVERIFY(!m.isEndpointFrame(QStringLiteral("pos"), 10));
    }
};

QTEST_MAIN(TestEffectModel)
#include "test_effect_model.moc"
