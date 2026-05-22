#include "effect_registry.hpp"
#include <QString>
#include <QTest>

using namespace AviQtl::Core;

class TestEffectRegistry : public QObject {
    Q_OBJECT

  private slots:
    void singletonInstance();
    void registerAndRetrieve();
    void duplicateRegistration();
    void getAllEffectsPreservesOrder();
    void getEffectNotFound();
};

void TestEffectRegistry::singletonInstance() {
    EffectRegistry &r1 = EffectRegistry::instance();
    EffectRegistry &r2 = EffectRegistry::instance();
    QCOMPARE(&r1, &r2);
}

void TestEffectRegistry::registerAndRetrieve() {
    EffectRegistry &reg = EffectRegistry::instance();

    EffectMetadata meta;
    meta.id = QStringLiteral("test.basic");
    meta.name = QStringLiteral("Basic Effect");
    meta.version = QStringLiteral("1.0.0");
    meta.kind = QStringLiteral("effect");
    meta.categories = {QStringLiteral("Test")};
    meta.qmlSource = QStringLiteral("test.qml");

    reg.registerEffect(meta);

    const EffectMetadata &fetched = reg.getEffect(QStringLiteral("test.basic"));
    QCOMPARE(fetched.id, QStringLiteral("test.basic"));
    QCOMPARE(fetched.name, QStringLiteral("Basic Effect"));
    QCOMPARE(fetched.version, QStringLiteral("1.0.0"));
    QCOMPARE(fetched.kind, QStringLiteral("effect"));
    QCOMPARE(fetched.qmlSource, QStringLiteral("test.qml"));
    QCOMPARE(fetched.categories.size(), 1);
    QCOMPARE(fetched.categories[0], QStringLiteral("Test"));
}

void TestEffectRegistry::duplicateRegistration() {
    EffectRegistry &reg = EffectRegistry::instance();

    EffectMetadata first;
    first.id = QStringLiteral("test.duplicate");
    first.name = QStringLiteral("First");
    first.version = QStringLiteral("1.0.0");
    first.kind = QStringLiteral("effect");
    first.categories = {QStringLiteral("Cat")};

    reg.registerEffect(first);

    EffectMetadata second;
    second.id = QStringLiteral("test.duplicate");
    second.name = QStringLiteral("Second");
    second.version = QStringLiteral("2.0.0");
    second.kind = QStringLiteral("object");
    second.categories = {QStringLiteral("New")};

    reg.registerEffect(second);

    const EffectMetadata &fetched = reg.getEffect(QStringLiteral("test.duplicate"));
    QCOMPARE(fetched.name, QStringLiteral("Second"));
    QCOMPARE(fetched.version, QStringLiteral("2.0.0"));
    QCOMPARE(fetched.kind, QStringLiteral("object"));
}

void TestEffectRegistry::getAllEffectsPreservesOrder() {
    EffectRegistry &reg = EffectRegistry::instance();

    EffectMetadata a, b, c;
    a.id = QStringLiteral("order.alpha");
    a.name = QStringLiteral("Alpha");
    a.version = QStringLiteral("1.0.0");
    a.kind = QStringLiteral("effect");
    a.categories = {QStringLiteral("Order")};

    b.id = QStringLiteral("order.beta");
    b.name = QStringLiteral("Beta");
    b.version = QStringLiteral("1.0.0");
    b.kind = QStringLiteral("effect");
    b.categories = {QStringLiteral("Order")};

    c.id = QStringLiteral("order.gamma");
    c.name = QStringLiteral("Gamma");
    c.version = QStringLiteral("1.0.0");
    c.kind = QStringLiteral("effect");
    c.categories = {QStringLiteral("Order")};

    reg.registerEffect(a);
    reg.registerEffect(b);
    reg.registerEffect(c);

    QList<EffectMetadata> all = reg.getAllEffects();

    int idxA = -1, idxB = -1, idxC = -1;
    for (int i = 0; i < all.size(); ++i) {
        if (all[i].id == QStringLiteral("order.alpha"))
            idxA = i;
        if (all[i].id == QStringLiteral("order.beta"))
            idxB = i;
        if (all[i].id == QStringLiteral("order.gamma"))
            idxC = i;
    }

    QVERIFY2(idxA >= 0, "order.alpha must be present in registry");
    QVERIFY2(idxB >= 0, "order.beta must be present in registry");
    QVERIFY2(idxC >= 0, "order.gamma must be present in registry");
    QVERIFY2(idxA < idxB, "Registration order must be preserved (alpha before beta)");
    QVERIFY2(idxB < idxC, "Registration order must be preserved (beta before gamma)");
}

void TestEffectRegistry::getEffectNotFound() {
    EffectRegistry &reg = EffectRegistry::instance();

    const EffectMetadata &fetched = reg.getEffect(QStringLiteral("__nonexistent__.uuid.1234"));
    QVERIFY(fetched.id.isEmpty());
    QVERIFY(fetched.name.isEmpty());
}

QTEST_MAIN(TestEffectRegistry)
#include "test_effect_registry.moc"
