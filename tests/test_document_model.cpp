#include "document_model.hpp"
#include <QSignalSpy>
#include <QTest>

using namespace AviQtl::Core;

class TestDocumentModel : public QObject {
    Q_OBJECT

  private slots:
    void init() { DocumentModel::instance().clear(); }

    void singletonInstance() {
        DocumentModel &m1 = DocumentModel::instance();
        DocumentModel &m2 = DocumentModel::instance();
        QCOMPARE(&m1, &m2);
    }

    void projectSettings() {
        DocumentModel &model = DocumentModel::instance();
        ProjectSettings expected;
        expected.name = QStringLiteral("Test Project");
        expected.defaultSceneWidth = 3840;
        expected.defaultSceneHeight = 2160;
        expected.defaultFps = 30.0;
        expected.audioSampleRate = 44100;
        expected.colorSpace = QStringLiteral("sRGB");

        model.setProjectSettings(expected);
        const ProjectSettings &actual = model.projectSettings();
        QCOMPARE(actual.name, expected.name);
        QCOMPARE(actual.defaultSceneWidth, expected.defaultSceneWidth);
        QCOMPARE(actual.defaultSceneHeight, expected.defaultSceneHeight);
        QCOMPARE(actual.defaultFps, expected.defaultFps);
        QCOMPARE(actual.audioSampleRate, expected.audioSampleRate);
        QCOMPARE(actual.colorSpace, expected.colorSpace);
    }

    void addAndFindScene() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 42;
        scene.name = QStringLiteral("Scene A");
        scene.width = 1280;
        scene.height = 720;
        scene.fps = 24.0;

        model.addScene(scene);
        const SceneSettings *found = model.findScene(42);
        QVERIFY(found != nullptr);
        QCOMPARE(found->name, QStringLiteral("Scene A"));
        QCOMPARE(found->width, 1280);
        QCOMPARE(found->height, 720);
        QCOMPARE(found->fps, 24.0);
    }

    void addSceneEmitsSignal() {
        DocumentModel &model = DocumentModel::instance();
        QSignalSpy spy(&model, &DocumentModel::structureChanged);

        SceneSettings scene;
        scene.id = 7;
        model.addScene(scene);

        QCOMPARE(spy.count(), 1);
    }

    void removeScene() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 99;
        model.addScene(scene);
        QVERIFY(model.findScene(99) != nullptr);

        model.removeScene(99);
        QVERIFY(model.findScene(99) == nullptr);
    }

    void removeSceneEmitsSignal() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 8;
        model.addScene(scene);

        QSignalSpy spy(&model, &DocumentModel::structureChanged);
        model.removeScene(8);

        QCOMPARE(spy.count(), 1);
    }

    void removeSceneNonExistent() {
        DocumentModel &model = DocumentModel::instance();
        QSignalSpy spy(&model, &DocumentModel::structureChanged);

        model.removeScene(99999); // does not exist
        QCOMPARE(spy.count(), 0);
        QVERIFY(model.findScene(99999) == nullptr);
    }

    void addAndFindClip() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 10;
        model.addScene(scene);

        Clip clip;
        clip.id = 101;
        clip.type = QStringLiteral("video");
        clip.layer = 2;
        clip.startFrame = 30;
        clip.durationFrames = 120;
        clip.sceneId = 10;

        model.addClip(10, clip);
        const Clip *found = model.findClip(10, 101);
        QVERIFY(found != nullptr);
        QCOMPARE(found->type, QStringLiteral("video"));
        QCOMPARE(found->layer, 2);
        QCOMPARE(found->startFrame, 30);
        QCOMPARE(found->durationFrames, 120);
    }

    void removeClip() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 11;
        model.addScene(scene);

        Clip clip;
        clip.id = 201;
        model.addClip(11, clip);
        QVERIFY(model.findClip(11, 201) != nullptr);

        model.removeClip(11, 201);
        QVERIFY(model.findClip(11, 201) == nullptr);
    }

    void removeClipNonExistent() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 12;
        model.addScene(scene);

        QSignalSpy spy(&model, &DocumentModel::structureChanged);
        model.removeClip(12, 99999);
        QCOMPARE(spy.count(), 0);
    }

    void clear() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 20;
        model.addScene(scene);
        QVERIFY(!model.scenes().empty());

        model.clear();
        QVERIFY(model.scenes().empty());
        QVERIFY(model.findScene(20) == nullptr);
    }

    void clearEmitsSignal() {
        DocumentModel &model = DocumentModel::instance();
        SceneSettings scene;
        scene.id = 21;
        model.addScene(scene);

        QSignalSpy spy(&model, &DocumentModel::structureChanged);
        model.clear();
        QCOMPARE(spy.count(), 1);
    }
};

QTEST_MAIN(TestDocumentModel)
#include "test_document_model.moc"
