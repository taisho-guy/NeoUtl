import QtQuick
import "common" as Common

Item {
    id: root

    property int sceneId: -1
    property int currentFrame: 0
    property var timelineBridge: null
    property var sceneStack: sceneId >= 0 ? [sceneId] : []
    property int sceneWidth: 1920
    property int sceneHeight: 1080
    // シーン情報を取得
    property var sceneInfo: {
        if (!timelineBridge || sceneId < 0)
            return null;

        var scenes = timelineBridge.scenes;
        for (var i = 0; i < scenes.length; i++) {
            if (scenes[i].id === sceneId)
                return scenes[i];

        }
        return null;
    }

    onSceneInfoChanged: {
        if (sceneInfo) {
            sceneWidth = sceneInfo.width || 1920;
            sceneHeight = sceneInfo.height || 1080;
        }
    }
    width: sceneWidth
    height: sceneHeight

    CompositeView {
        id: compositeView

        anchors.fill: parent
        // 外部から注入されたデータを使用
        clipModel: {
            if (root.timelineBridge && root.sceneId >= 0)
                return root.timelineBridge.getSceneClips(root.sceneId);

            return [];
        }
        sceneId: root.sceneId
        sceneStack: root.sceneStack
        projectWidth: root.sceneWidth
        projectHeight: root.sceneHeight
        currentFrame: root.currentFrame
        layerStates: {
            var tlWin = WindowManager.getWindow("timeline");
            return tlWin ? tlWin.globalLayerStates : ({
            });
        }

        Connections {
            function onGlobalLayerStatesChanged() {
                compositeView.layerStates = WindowManager.getWindow("timeline").globalLayerStates;
            }

            target: WindowManager.getWindow("timeline")
        }

    }

}
