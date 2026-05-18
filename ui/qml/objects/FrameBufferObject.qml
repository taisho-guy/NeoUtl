import QtQml
import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    // CompositeView から注入 (properties ではなく onItemChanged で動的セット)
    property var sceneRootRef: null
    // onItemChanged で item.clipLayer = model.layer される
    property bool clearBelow: Boolean(evalParam("frame_buffer", "clearBelow", false))
    // ObjectRenderer の Binding が要求するダミープロパティ (警告抑制)
    property var source: undefined
    property var params: ({
    })
    property var effectModel: null
    property int frame: 0
    // width/height は flattenHost のサイズで代替 (Binding が上書きしないよう明示宣言)
    property real fbWidth: flattenHost.width
    property real fbHeight: flattenHost.height
    // ─── 内部: 上位レイヤー収集 ───────────────────────────────────
    property var _capturedOutputs: []

    function _rebuildCapture() {
        if (!sceneRootRef || clipLayer < 0) {
            _capturedOutputs = [];
            return ;
        }
        var outputs = [];
        var ch = sceneRootRef.children;
        for (var i = 0; i < ch.length; i++) {
            var node = ch[i];
            var nodeLayer = (node.clipLayerRole !== undefined) ? node.clipLayerRole : -1;
            if (nodeLayer >= 0 && nodeLayer < root.clipLayer) {
                var out = node.fbRendererOutput;
                if (out)
                    outputs.push({
                    "layer": nodeLayer,
                    "src": out
                });

            }
        }
        // レイヤー昇順ソートして output だけ抽出
        outputs.sort(function(a, b) {
            return a.layer - b.layer;
        });
        var sorted = [];
        for (var j = 0; j < outputs.length; j++) sorted.push(outputs[j].src)
        _capturedOutputs = sorted;
    }

    onSceneRootRefChanged: Qt.callLater(root._rebuildCapture)
    onClipLayerChanged: Qt.callLater(root._rebuildCapture)
    Component.onCompleted: {
        adopt2D(flattenHost);
        adopt2D(fbSourceWrapper);
        fbSourceWrapper.visible = true; // BaseObject.onSourceItemChanged が false にするため上書き
        Qt.callLater(_rebuildCapture);
    }

    // CompositeView 側からの集中通知を一括で受け取る
    Connections {
        function onChildRendererOutputsChanged() {
            Qt.callLater(root._rebuildCapture);
        }

        // root.renderHost (offscreenRenderHost) の親は常に CompositeView
        target: (root.renderHost && root.renderHost.parent) ? root.renderHost.parent : null
        ignoreUnknownSignals: true
    }

    Connections {
        function onClipsChanged() {
            Qt.callLater(root._rebuildCapture);
        }

        target: Workspace.currentTimeline
    }

    // ─── 合成ホスト (offscreenRenderHost へ adopt2D される) ──────
    Item {
        id: flattenHost

        width: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.width : 1920
        height: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.height : 1080
        visible: true // SceneGraph に残す (renderHost 側の opacity:0 で非表示にする)

        // ダミーの透明背景
        Item {
            id: dummyBackground

            anchors.fill: parent
            visible: false
        }

        // 連鎖的ブレンド合成チェーン
        Repeater {
            id: blendChain

            model: root._capturedOutputs

            Loader {
                id: layerLoader

                property Item prevOutput: {
                    if (index === 0)
                        return dummyBackground;

                    var prev = blendChain.itemAt(index - 1);
                    return prev ? prev.item.output : dummyBackground;
                }

                anchors.fill: parent
                active: true
                sourceComponent: blendEffectComponent
            }

        }

    }

    Component {
        id: blendEffectComponent

        Item {
            id: blendEffectItem

            // この段階の合成結果テクスチャを公開
            property alias output: resultCapture
            property Item backgroundItem: prevOutput
            property Item foregroundItem: modelData

            anchors.fill: parent

            ShaderEffect {
                id: effect

                property variant background

                background: ShaderEffectSource {
                    sourceItem: backgroundItem
                    live: true
                    hideSource: false
                }

                property variant source

                source: ShaderEffectSource {
                    sourceItem: foregroundItem
                    live: true
                    hideSource: false
                }

                // 前景（レイヤー）が公開しているブレンドパラメータを注入
                property int blendMode: foregroundItem ? (foregroundItem.blendMode || 0) : 0
                property real opacityValue: foregroundItem ? (foregroundItem.opacityValue !== undefined ? foregroundItem.opacityValue : 1) : 1

                anchors.fill: parent
                fragmentShader: "../effects/blend_layer.frag.qsb"
            }

            ShaderEffectSource {
                id: resultCapture

                anchors.fill: parent
                sourceItem: effect
                live: true
                hideSource: true
            }

        }

    }

    // ─── 3D Model として View3D に配置 ───────────────────────────
    Model {
        source: "#Rectangle"
        scale: Qt.vector3d(flattenHost.width / 100, flattenHost.height / 100, 1)

        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            blendMode: root.blendMode
            cullMode: root.cullMode

            diffuseMap: Texture {
                sourceItem: renderer.output
            }

        }

    }

    // clearBelow: 下位レイヤーを黒でマスク
    Rectangle {
        visible: root.clearBelow
        anchors.fill: parent
        color: "black"
        z: -1
    }

    // flattenHost またはブレンドチェーンの最終結果を1枚のテクスチャに焼く → sourceItem
    sourceItem: Item {
        id: fbSourceWrapper

        width: flattenHost.width
        height: flattenHost.height
        visible: true // visible:false だと ShaderEffectSource の更新が止まる

        ShaderEffectSource {
            anchors.fill: parent
            sourceItem: {
                if (blendChain.count > 0) {
                    var last = blendChain.itemAt(blendChain.count - 1);
                    return last ? last.item.output : flattenHost;
                }
                return flattenHost;
            }
            live: true
            hideSource: true
        }

    }

}
