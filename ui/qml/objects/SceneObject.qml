import QtQuick
import QtQuick.Shapes
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml" as Ui
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property int targetSceneId: evalParam("scene", "targetSceneId", -1)
    property real speed: evalParam("scene", "speed", 1)
    property int offset: evalParam("scene", "offset", 0)
    property real opacity: evalParam("scene", "opacity", 1)
    property var sceneStack: []
    readonly property bool recursiveReference: {
        if (targetSceneId < 0)
            return true;

        for (var i = 0; i < sceneStack.length; i++) {
            if (sceneStack[i] === targetSceneId)
                return true;

        }
        return false;
    }
    // シーン内時間計算
    property int sceneFrame: {
        var f = Math.floor(relFrame * speed) + offset;
        // シーン長が定義されていればクランプ
        var dur = (!recursiveReference && typeof Workspace.currentTimeline !== "undefined") ? Workspace.currentTimeline.getSceneDuration(targetSceneId) : 0;
        if (dur > 0)
            f = Math.max(0, Math.min(f, dur - 1));

        return f;
    }

    // 3Dモデルとして表示
    Model {
        source: "#Rectangle"
        scale: Qt.vector3d(root.sourceItem.width / 100, root.sourceItem.height / 100, 1)
        opacity: root.opacity
        visible: !root.recursiveReference

        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            blendMode: root.blendMode
            cullMode: root.cullMode

            diffuseMap: Texture {
                sourceItem: renderer.output
            }

        }

    }

    // 以前のImage/SceneDecoderベースの代わりに、SceneRendererを直接組み込む
    // GPU空間内でシーングラフとして完結させる
    sourceItem: Ui.SceneRenderer {
        sceneId: root.recursiveReference ? -1 : root.targetSceneId
        currentFrame: root.sceneFrame
        timelineBridge: typeof Workspace.currentTimeline !== "undefined" ? Workspace.currentTimeline : null
        sceneStack: root.recursiveReference ? root.sceneStack : root.sceneStack.concat([root.targetSceneId])
        visible: false // テクスチャソースとして用いるため可視化しない
    }

}
