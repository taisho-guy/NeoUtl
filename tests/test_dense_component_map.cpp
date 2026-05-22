#include "engine/timeline/ecs.hpp"
#include <QTest>

using namespace AviQtl::Engine::Timeline;

class TestDenseComponentMap : public QObject {
    Q_OBJECT

  private slots:
    void operatorBracketCreatesElement() {
        DenseComponentMap<TransformComponent> map;
        TransformComponent &t = map[5];
        QCOMPARE(t.layer, 0);
        QCOMPARE(t.timePosition, 0.0);

        // Verify find returns the same element
        QVERIFY(map.contains(5));
        QCOMPARE(map.find(5), &t);
    }

    void findMissingReturnsNull() {
        DenseComponentMap<TransformComponent> map;
        QVERIFY(map.find(0) == nullptr);
        QVERIFY(map.find(1000) == nullptr);
    }

    void contains() {
        DenseComponentMap<TransformComponent> map;
        QVERIFY(!map.contains(3));
        map[3] = TransformComponent{};
        QVERIFY(map.contains(3));
    }

    void eraseRemovesElement() {
        DenseComponentMap<TransformComponent> map;
        map[1] = TransformComponent{};
        map[2] = TransformComponent{};
        QVERIFY(map.contains(1));
        QVERIFY(map.contains(2));

        map.erase(1);
        QVERIFY(!map.contains(1));
        QVERIFY(map.contains(2));
    }

    void eraseNonExistentIsNoOp() {
        DenseComponentMap<TransformComponent> map;
        map.erase(999); // should not crash
        QVERIFY(!map.contains(999));
    }

    void iteration() {
        DenseComponentMap<TransformComponent> map;
        map[10] = TransformComponent{};
        map[10].layer = 10;
        map[20] = TransformComponent{};
        map[20].layer = 20;

        int count = 0;
        for (auto it = map.begin(); it != map.end(); ++it) {
            ++count;
        }
        QCOMPARE(count, 2);
    }

    void forEach() {
        DenseComponentMap<TransformComponent> map;
        map[5] = TransformComponent{};
        map[5].layer = 50;
        map[7] = TransformComponent{};
        map[7].layer = 70;

        int sum = 0;
        map.forEach([&sum](int /*id*/, const TransformComponent &tc) { sum += tc.layer; });
        QCOMPARE(sum, 120);
    }

    void syncAliveRemovesDead() {
        DenseComponentMap<TransformComponent> map;
        std::bitset<MAX_CLIP_ID> alive;

        // Create 3 elements with IDs 1, 2, 3
        map[1] = TransformComponent{};
        map[2] = TransformComponent{};
        map[3] = TransformComponent{};
        QVERIFY(map.contains(1));
        QVERIFY(map.contains(2));
        QVERIFY(map.contains(3));

        // Mark only 1 and 3 as alive
        alive.set(1);
        alive.set(3);

        bool changed = map.syncAlive(alive);
        QVERIFY(changed);
        QVERIFY(map.contains(1));
        QVERIFY(!map.contains(2));
        QVERIFY(map.contains(3));
    }

    void syncAliveNoChange() {
        DenseComponentMap<TransformComponent> map;
        std::bitset<MAX_CLIP_ID> alive;

        map[10] = TransformComponent{};
        alive.set(10);

        bool changed = map.syncAlive(alive);
        QVERIFY(!changed);
        QVERIFY(map.contains(10));
    }

    void multipleTypes() {
        DenseComponentMap<TransformComponent> tmap;
        DenseComponentMap<AudioComponent> amap;

        tmap[1] = TransformComponent{};
        tmap[1].layer = 5;
        amap[1] = AudioComponent{};
        amap[1].volume = 0.5f;

        QCOMPARE(tmap.find(1)->layer, 5);
        QCOMPARE(amap.find(1)->volume, 0.5f);
    }

    void denseStorageCompaction() {
        DenseComponentMap<TransformComponent> map;
        map[0] = TransformComponent{};
        map[0].layer = 100;
        map[1] = TransformComponent{};
        map[1].layer = 200;
        map[2] = TransformComponent{};
        map[2].layer = 300;

        // Erase middle element
        map.erase(1);

        // Remaining elements should still be accessible
        QCOMPARE(map.find(0)->layer, 100);
        QCOMPARE(map.find(2)->layer, 300);
        QVERIFY(map.find(1) == nullptr);
    }
};

QTEST_MAIN(TestDenseComponentMap)
#include "test_dense_component_map.moc"
