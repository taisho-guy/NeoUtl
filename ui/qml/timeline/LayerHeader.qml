import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: headerRoot

    property int headerWidth: 60
    property int layerHeight: 30
    property int layerCount: 128
    property var syncFlickable: null // TimelineViewのFlickable
    // レイヤー状態管理 (内部保持)
    property var _layerStates: ({
    })

    // 外部への通知シグナル
    signal layerVisibilityChanged(int layer, bool visible)
    signal layerLockChanged(int layer, bool locked)

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function getLayerVisible(layer) {
        var state = _layerStates[layer];
        return state ? state.visible : true;
    }

    function getLayerLocked(layer) {
        var state = _layerStates[layer];
        return state ? state.locked : false;
    }

    function setLayerVisible(layer, visible) {
        if (!_layerStates[layer])
            _layerStates[layer] = {
            "visible": true,
            "locked": false
        };

        _layerStates[layer].visible = visible;
        layerVisibilityChanged(layer, visible);
        // 強制的に再評価を促す
        _layerStatesChanged();
    }

    function setLayerLocked(layer, locked) {
        if (!_layerStates[layer])
            _layerStates[layer] = {
            "visible": true,
            "locked": false
        };

        _layerStates[layer].locked = locked;
        layerLockChanged(layer, locked);
        _layerStatesChanged();
    }

    Layout.preferredWidth: headerWidth
    Layout.fillHeight: true
    color: palette.button
    z: 2

    Flickable {
        id: layerHeaderFlickable

        anchors.fill: parent
        contentHeight: layerCount * layerHeight
        // TimelineViewと同期
        contentY: syncFlickable ? syncFlickable.contentY : 0
        interactive: false // 独自のホイール処理を行うため
        clip: true

        Column {
            Repeater {
                model: layerCount

                // レイヤーボタン (AviUtl風)
                Button {
                    id: layerBtn

                    property int layerIndex: index
                    property bool isVisible: headerRoot.getLayerVisible(layerIndex)
                    property bool isLocked: headerRoot.getLayerLocked(layerIndex)
                    property bool isSelected: (Workspace.currentTimeline && Workspace.currentTimeline.selectedLayer === layerIndex)

                    width: headerRoot.headerWidth
                    height: headerRoot.layerHeight
                    flat: true
                    // 左クリック: 表示/非表示トグル
                    onClicked: {
                        headerRoot.setLayerVisible(layerIndex, !isVisible);
                        if (Workspace.currentTimeline)
                            Workspace.currentTimeline.selectedLayer = layerIndex;

                    }

                    // 右クリック: コンテキストメニュー
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        acceptedButtons: Qt.RightButton
                        onClicked: {
                            layerMenu.layerIndex = layerBtn.layerIndex;
                            layerMenu.popup();
                        }
                    }

                    // 背景色: 非表示時は暗く、ロック時は赤みがかる
                    background: Rectangle {
                        color: {
                            if (!layerBtn.isVisible)
                                return Qt.darker(palette.button, 1.5);

                            if (layerBtn.isLocked)
                                return Qt.rgba(0.6, 0.3, 0.3, 1);

                            var base = (layerBtn.layerIndex % 2 == 0) ? palette.button : Qt.darker(palette.button, 1.1);
                            return layerBtn.isSelected ? palette.highlight : base;
                        }
                        border.color: layerBtn.isSelected ? palette.highlight : palette.mid
                        border.width: layerBtn.isSelected ? 2 : 1
                    }

                    // レイヤー番号表示
                    contentItem: Item {
                        Text {
                            anchors.centerIn: parent
                            text: (layerBtn.layerIndex + 1).toString()
                            color: {
                                if (!layerBtn.isVisible)
                                    return palette.mid;

                                if (layerBtn.isLocked)
                                    return "#ffcccc";

                                return layerBtn.isSelected ? palette.highlightedText : palette.text;
                            }
                            font.pixelSize: 12
                            font.bold: (layerBtn.isVisible && !layerBtn.isLocked) || layerBtn.isSelected
                        }

                        // 状態インジケーター (右上に小さな記号)
                        Row {
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 2
                            spacing: 2

                            Common.AviQtlIcon {
                                visible: layerBtn.isLocked
                                iconName: "lock_fill"
                                size: 10
                                color: "#ffcccc"
                            }

                            Common.AviQtlIcon {
                                visible: !layerBtn.isVisible
                                iconName: "eye_off_line"
                                size: 10
                                color: palette.mid
                            }

                        }

                    }

                }

            }

        }

    }

    // レイヤーコンテキストメニュー
    Menu {
        id: layerMenu

        property int layerIndex: 0

        Common.IconMenuItem {
            text: qsTr("表示/非表示を切り替え")
            iconName: headerRoot.getLayerVisible(layerMenu.layerIndex) ? "eye_off_line" : "eye_line"
            onTriggered: {
                var visible = headerRoot.getLayerVisible(layerMenu.layerIndex);
                headerRoot.setLayerVisible(layerMenu.layerIndex, !visible);
            }
        }

        MenuSeparator {
        }

        Common.IconMenuItem {
            text: {
                var locked = headerRoot.getLayerLocked(layerMenu.layerIndex);
                return locked ? qsTr("ロックを解除") : qsTr("ロック");
            }
            iconName: headerRoot.getLayerLocked(layerMenu.layerIndex) ? "lock_unlock_line" : "lock_line"
            onTriggered: {
                var locked = headerRoot.getLayerLocked(layerMenu.layerIndex);
                headerRoot.setLayerLocked(layerMenu.layerIndex, !locked);
            }
        }

        MenuSeparator {
        }

        Common.IconMenuItem {
            text: qsTr("すべて表示")
            iconName: "eye_line"
            onTriggered: {
                for (var i = 0; i < headerRoot.layerCount; i++) {
                    headerRoot.setLayerVisible(i, true);
                }
            }
        }

        Common.IconMenuItem {
            text: qsTr("すべて非表示")
            iconName: "eye_off_line"
            onTriggered: {
                for (var i = 0; i < headerRoot.layerCount; i++) {
                    headerRoot.setLayerVisible(i, false);
                }
            }
        }

    }

    // 縦スクロール専用マウスエリア
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
        z: -1
        hoverEnabled: true
        onWheel: (wheel) => {
            if (!syncFlickable)
                return ;

            var dy = wheel.angleDelta.y;
            if (wheel.pixelDelta && wheel.pixelDelta.y !== 0)
                dy = wheel.pixelDelta.y * 10;

            var nextY = syncFlickable.contentY - dy;
            var maxY = Math.max(0, syncFlickable.contentHeight - syncFlickable.height);
            syncFlickable.contentY = clamp(nextY, 0, maxY);
            wheel.accepted = true;
        }
    }

}
