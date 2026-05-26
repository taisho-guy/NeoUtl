import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    // 強制的に再評価を促す

    id: headerRoot

    property int headerWidth: 60
    property int layerHeight: 30
    property int layerCount: 128
    property var syncFlickable: null // TimelineViewのFlickable
    // レイヤー状態管理 (内部保持)
    property int layerStateRevision: 0

    // 外部への通知シグナル
    signal layerVisibilityChanged(int layer, bool visible)
    signal layerLockChanged(int layer, bool locked)

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function getLayerVisible(layer) {
        if (!Workspace.currentTimeline || typeof Workspace.currentTimeline.isLayerHidden !== "function")
            return true;

        return !Workspace.currentTimeline.isLayerHidden(layer);
    }

    function getLayerLocked(layer) {
        if (!Workspace.currentTimeline || typeof Workspace.currentTimeline.isLayerLocked !== "function")
            return false;

        return Workspace.currentTimeline.isLayerLocked(layer);
    }

    function setLayerVisible(layer, visible) {
        if (Workspace.currentTimeline && typeof Workspace.currentTimeline.setLayerState === "function")
            Workspace.currentTimeline.setLayerState(layer, !visible, 1);

        layerVisibilityChanged(layer, visible);
        layerStateRevision++;
    }

    function setLayerLocked(layer, locked) {
        if (Workspace.currentTimeline && typeof Workspace.currentTimeline.setLayerState === "function")
            Workspace.currentTimeline.setLayerState(layer, locked, 0);

        layerLockChanged(layer, locked);
        layerStateRevision++;
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
                    property bool isVisible: (headerRoot.layerStateRevision, headerRoot.getLayerVisible(layerIndex))
                    property bool isLocked: (headerRoot.layerStateRevision, headerRoot.getLayerLocked(layerIndex))
                    property bool isSelected: (Workspace.currentTimeline && Workspace.currentTimeline.selectedLayer === layerIndex)

                    width: headerRoot.headerWidth
                    height: headerRoot.layerHeight
                    flat: true
                    // 左クリック: 表示/非表示トグル
                    onClicked: () => {
                        headerRoot.setLayerVisible(layerIndex, !isVisible);
                        if (Workspace.currentTimeline)
                            Workspace.currentTimeline.selectedLayer = layerIndex;

                    }

                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        acceptedButtons: Qt.RightButton
                        onClicked: {
                            if (Workspace.currentTimeline)
                                Workspace.currentTimeline.selectedLayer = layerBtn.layerIndex;

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

    // 複数レイヤー挿入用ダイアログ
    Connections {
        function onCurrentTimelineChanged() {
            headerRoot.layerStateRevision++;
        }

        target: Workspace
    }

    Connections {
        function onClipsChanged() {
            headerRoot.layerStateRevision++;
        }

        function onCurrentSceneIdChanged() {
            headerRoot.layerStateRevision++;
        }

        function onScenesChanged() {
            headerRoot.layerStateRevision++;
        }

        target: Workspace.currentTimeline
    }

    Dialog {
        id: insertLayersDialog

        property bool isAbove: true

        anchors.centerIn: Overlay.overlay
        title: qsTr("複数レイヤーを挿入")
        standardButtons: Dialog.Ok | Dialog.Cancel
        modal: true
        onAccepted: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.insertLayers(layerMenu.layerIndex, countSpin.value, isAbove);

        }

        ColumnLayout {
            spacing: 10

            Label {
                text: qsTr("挿入するレイヤー数:")
            }

            SpinBox {
                id: countSpin

                from: 1
                to: 100
                value: 5
                editable: true
            }

            Label {
                text: qsTr("挿入方向:")
            }

            RowLayout {
                RadioButton {
                    text: qsTr("選択レイヤーの上")
                    checked: insertLayersDialog.isAbove
                    onToggled: {
                        if (checked)
                            insertLayersDialog.isAbove = true;

                    }
                }

                RadioButton {
                    text: qsTr("選択レイヤーの下")
                    checked: !insertLayersDialog.isAbove
                    onToggled: {
                        if (checked)
                            insertLayersDialog.isAbove = false;

                    }
                }

            }

        }

    }

    // 複数レイヤー移動用ダイアログ
    Dialog {
        id: moveLayersDialog

        property bool isDown: true

        anchors.centerIn: Overlay.overlay
        title: qsTr("このレイヤー以降をまとめて移動")
        standardButtons: Dialog.Ok | Dialog.Cancel
        modal: true
        onAccepted: {
            if (Workspace.currentTimeline) {
                var delta = isDown ? moveCountSpin.value : -moveCountSpin.value;
                Workspace.currentTimeline.shiftLayers(fromLayerSpin.value - 1, toLayerSpin.value - 1, delta);
            }
        }

        ColumnLayout {
            spacing: 10

            Label {
                text: qsTr("対象レイヤーの範囲:")
            }

            RowLayout {
                SpinBox {
                    id: fromLayerSpin

                    from: 1
                    to: headerRoot.layerCount
                    value: layerMenu.layerIndex + 1
                    editable: true
                }

                Label {
                    text: "～"
                }

                SpinBox {
                    id: toLayerSpin

                    from: 1
                    to: headerRoot.layerCount
                    value: headerRoot.layerCount
                    editable: true
                }

            }

            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: palette.mid
                opacity: 0.3
            }

            Label {
                text: qsTr("移動量 (行数):")
            }

            SpinBox {
                id: moveCountSpin

                from: 1
                to: 100
                value: 1
                editable: true
            }

            Label {
                text: qsTr("方向:")
            }

            RowLayout {
                RadioButton {
                    text: qsTr("上へ")
                    checked: !moveLayersDialog.isDown
                    onToggled: {
                        if (checked)
                            moveLayersDialog.isDown = false;

                    }
                }

                RadioButton {
                    text: qsTr("下へ")
                    checked: moveLayersDialog.isDown
                    onToggled: {
                        if (checked)
                            moveLayersDialog.isDown = true;

                    }
                }

            }

        }

    }

    Menu {
        id: layerMenu

        property int layerIndex: 0

        // --- 挿入系 ---
        MenuSeparator {
        }

        Common.IconMenuItem {
            text: qsTr("上にレイヤーを挿入 (1行)")
            iconName: "add_line"
            onTriggered: () => {
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.insertLayers(layerMenu.layerIndex, 1, true);

            }
        }

        Common.IconMenuItem {
            text: qsTr("下にレイヤーを挿入 (1行)")
            iconName: "add_line"
            onTriggered: () => {
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.insertLayers(layerMenu.layerIndex, 1, false);

            }
        }

        Common.IconMenuItem {
            text: qsTr("複数レイヤーを挿入...")
            iconName: "apps_2_add_line"
            onTriggered: insertLayersDialog.open()
        }

        // --- 移動系 ---
        MenuSeparator {
        }

        Common.IconMenuItem {
            text: qsTr("このレイヤーの内容を1行下へ")
            iconName: "arrow_down_line"
            onTriggered: {
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.shiftLayers(layerMenu.layerIndex, layerMenu.layerIndex, 1);

            }
        }

        Common.IconMenuItem {
            text: qsTr("このレイヤーの内容を1行上へ")
            iconName: "arrow_up_line"
            onTriggered: {
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.shiftLayers(layerMenu.layerIndex, layerMenu.layerIndex, -1);

            }
        }

        Common.IconMenuItem {
            text: qsTr("範囲を指定してレイヤー移動...")
            iconName: "drag_move_2_line"
            onTriggered: moveLayersDialog.open()
        }

        MenuSeparator {
        }

        // --- 状態設定系 ---
        Common.IconMenuItem {
            text: headerRoot.getLayerVisible(layerMenu.layerIndex) ? qsTr("このレイヤーを非表示にする") : qsTr("このレイヤーを表示する")
            iconName: headerRoot.getLayerVisible(layerMenu.layerIndex) ? "eye_off_line" : "eye_line"
            onTriggered: {
                var visible = headerRoot.getLayerVisible(layerMenu.layerIndex);
                headerRoot.setLayerVisible(layerMenu.layerIndex, !visible);
            }
        }

        Common.IconMenuItem {
            text: {
                var locked = headerRoot.getLayerLocked(layerMenu.layerIndex);
                return locked ? qsTr("このレイヤーのロックを解除") : qsTr("このレイヤーをロックする");
            }
            iconName: headerRoot.getLayerLocked(layerMenu.layerIndex) ? "lock_unlock_line" : "lock_line"
            onTriggered: {
                var locked = headerRoot.getLayerLocked(layerMenu.layerIndex);
                headerRoot.setLayerLocked(layerMenu.layerIndex, !locked);
            }
        }

        // --- 一括操作系 ---
        MenuSeparator {
        }

        Common.IconMenuItem {
            text: qsTr("すべてのレイヤーを表示")
            iconName: "eye_line"
            onTriggered: {
                for (var i = 0; i < headerRoot.layerCount; i++) {
                    headerRoot.setLayerVisible(i, true);
                }
            }
        }

        Common.IconMenuItem {
            text: qsTr("すべてのレイヤーを非表示")
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
