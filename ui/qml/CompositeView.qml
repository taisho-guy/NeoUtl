import QtQuick

// CompositeViewはSceneRendererを内包するコンテナ。
// 描画の実体はFilamentCanvas(C++)が担う。
// クリップモデルへの参照・ObjectRenderer・エフェクトチェーンは全廃。
Item {
    id: root

    property int sceneId: -1
    property int currentFrame: 0
    property int projectWidth: 1920
    property int projectHeight: 1080
    property var layerStates: ({
    })

    width: projectWidth
    height: projectHeight

    SceneRenderer {
        anchors.fill: parent
        sceneId: root.sceneId
        currentFrame: root.currentFrame
        sceneWidth: root.projectWidth
        sceneHeight: root.projectHeight
    }

}
