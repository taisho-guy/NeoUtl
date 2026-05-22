#include "selection_service.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::UI;

class TestSelectionService : public QObject {
    Q_OBJECT

  private slots:
    void initialState() {
        SelectionService svc;
        QCOMPARE(svc.selectedClipId(), -1);
        QVERIFY(svc.selectedClipIds().isEmpty());
        QVERIFY(!svc.isSelected(0));
        QVERIFY(!svc.isSelected(-1));
    }

    void selectSingle() {
        SelectionService svc;
        QSignalSpy idsSpy(&svc, &SelectionService::selectedClipIdsChanged);
        QSignalSpy primarySpy(&svc, &SelectionService::selectedClipIdChanged);
        QSignalSpy dataSpy(&svc, &SelectionService::selectedClipDataChanged);

        QVariantMap data;
        data.insert(QStringLiteral("name"), QStringLiteral("Clip A"));
        svc.select(1, data);

        QCOMPARE(svc.selectedClipId(), 1);
        QCOMPARE(svc.selectedClipIds().size(), 1);
        QVERIFY(svc.isSelected(1));
        QCOMPARE(idsSpy.count(), 1);
        QCOMPARE(primarySpy.count(), 1);
        QCOMPARE(dataSpy.count(), 1);
    }

    void selectReplacesPrevious() {
        SelectionService svc;
        svc.select(1, QVariantMap());
        svc.select(2, QVariantMap());

        QCOMPARE(svc.selectedClipId(), 2);
        QVERIFY(!svc.isSelected(1));
        QVERIFY(svc.isSelected(2));
    }

    void toggleSelection() {
        SelectionService svc;
        svc.toggleSelection(1, QVariantMap());
        QVERIFY(svc.isSelected(1));

        svc.toggleSelection(2, QVariantMap());
        QVERIFY(svc.isSelected(1));
        QVERIFY(svc.isSelected(2));
        QCOMPARE(svc.selectedClipId(), 2); // last toggled becomes primary
    }

    void toggleDeselect() {
        SelectionService svc;
        svc.toggleSelection(1, QVariantMap());
        QVERIFY(svc.isSelected(1));

        svc.toggleSelection(1, QVariantMap());
        QVERIFY(!svc.isSelected(1));
        QCOMPARE(svc.selectedClipId(), -1);
    }

    void toggleNegativeIdClears() {
        SelectionService svc;
        svc.select(1, QVariantMap());
        svc.toggleSelection(-1, QVariantMap());
        QVERIFY(svc.selectedClipIds().isEmpty());
        QCOMPARE(svc.selectedClipId(), -1);
    }

    void clearSelection() {
        SelectionService svc;
        svc.select(1, QVariantMap());
        svc.select(2, QVariantMap());

        QSignalSpy idsSpy(&svc, &SelectionService::selectedClipIdsChanged);
        svc.clearSelection();

        QVERIFY(svc.selectedClipIds().isEmpty());
        QCOMPARE(svc.selectedClipId(), -1);
        QCOMPARE(idsSpy.count(), 1);
    }

    void clearOnEmptyNoSignal() {
        SelectionService svc;
        QSignalSpy idsSpy(&svc, &SelectionService::selectedClipIdsChanged);
        svc.clearSelection();
        QCOMPARE(idsSpy.count(), 0);
    }

    void replaceSelectionBulk() {
        SelectionService svc;
        svc.select(1, QVariantMap());
        svc.select(2, QVariantMap());

        QVariantList ids;
        ids.append(5);
        ids.append(6);
        svc.replaceSelection(ids, 5, QVariantMap());

        QVERIFY(!svc.isSelected(1));
        QVERIFY(!svc.isSelected(2));
        QVERIFY(svc.isSelected(5));
        QVERIFY(svc.isSelected(6));
        QCOMPARE(svc.selectedClipId(), 5);
    }

    void replaceSelectionDeduplicates() {
        SelectionService svc;
        QVariantList ids;
        ids.append(1);
        ids.append(1); // duplicate
        ids.append(2);
        svc.replaceSelection(ids, 1, QVariantMap());

        QCOMPARE(svc.selectedClipIds().size(), 2); // 1 and 2
    }

    void refreshSelectionData() {
        SelectionService svc;
        QVariantMap oldData;
        oldData.insert(QStringLiteral("name"), QStringLiteral("Old"));
        svc.select(1, oldData);

        QSignalSpy dataSpy(&svc, &SelectionService::selectedClipDataChanged);
        QVariantMap newData;
        newData.insert(QStringLiteral("name"), QStringLiteral("New"));
        svc.refreshSelectionData(1, newData);

        QCOMPARE(dataSpy.count(), 1);
    }

    void refreshSelectionDataNonSelected() {
        SelectionService svc;
        QSignalSpy dataSpy(&svc, &SelectionService::selectedClipDataChanged);
        svc.refreshSelectionData(1, QVariantMap());
        QCOMPARE(dataSpy.count(), 0);
    }
};

QTEST_MAIN(TestSelectionService)
#include "test_selection_service.moc"
