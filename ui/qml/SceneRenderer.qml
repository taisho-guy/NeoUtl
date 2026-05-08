import AviQtl.Rendering 1.0
import QtQuick

// SceneRendererはFilamentCanvasの薄いラッパー。
// クリップの問い合わせや描画制御はC++コア(ECS RenderSystem)が担う。
Item {
    id: root

    property int sceneId: -1
    property int currentFrame: 0
    property int sceneWidth: 1920
    property int sceneHeight: 1080

    width: sceneWidth
    height: sceneHeight

    FilamentCanvas {
        id: canvas

        anchors.fill: parent
        sceneId: root.sceneId
        currentFrame: root.currentFrame
    }

}
