import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common
import "timeline" // サブフォルダのモジュールをインポート

Common.AviQtlWindow {
    // ─── タイムライン専用ショートカット (WindowShortcut) ───
    // これにより、メインウィンドウや設定ダイアログとの競合を避けつつ、確実に動作させます。

    id: timelineWindow

    // 定数・設定
    property var settings: SettingsManager.settings
    readonly property int layerCount: settings.timelineMaxLayers || 128
    readonly property int layerHeight: settings.timelineTrackHeight || 30
    readonly property int rulerHeight: settings.timelineRulerHeight || 32
    readonly property int headerWidth: settings.timelineLayerHeaderWidth || 60
    readonly property int clipResizeHandleWidth: settings.timelineClipResizeHandleWidth || 10
    readonly property int sceneTabHeight: settings.timelineHeaderHeight || 28
    // レイヤー状態のグローバル管理（LayerHeaderからの通知を受け取る）
    property var globalLayerStates: ({
    })
    // 入力フォーカス判定
    readonly property bool _isInputFocused: {
        var item = Qt.application.focusItem;
        if (!item)
            return false;

        // フォーカスを持つアイテムがない場合は入力中ではない
        return item.hasOwnProperty("echoMode") || (item.hasOwnProperty("selectionStart") && item.readOnly === false);
    }

    function getLayerVisible(layer) {
        var state = globalLayerStates[layer];
        return state ? state.visible : true;
    }

    title: qsTr("タイムライン")
    objectName: "timelineWindow"
    width: 1280
    height: 300

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // 1. シーンタブ
        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: sceneTabHeight
            spacing: 0
            z: 1

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                TabBar {
                    id: sceneTabBar

                    // ScrollView内で適切に広がるように設定
                    width: Math.max(parent.width, contentWidth)

                    Repeater {
                        id: sceneRepeater

                        model: Workspace.currentTimeline ? Workspace.currentTimeline.scenes : []

                        TabButton {
                            id: tabBtn

                            implicitWidth: Math.max(100, contentItem.implicitWidth + leftPadding + rightPadding)
                            checked: Workspace.currentTimeline && Workspace.currentTimeline.currentSceneId === modelData.id
                            onClicked: {
                                if (Workspace.currentTimeline)
                                    Workspace.currentTimeline.switchScene(modelData.id);

                            }

                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.RightButton
                                onClicked: {
                                    var win = WindowManager.getWindow("sceneSettings");
                                    if (win)
                                        win.openForScene(modelData.id, modelData.name, modelData.width !== undefined ? modelData.width : 1920, modelData.height !== undefined ? modelData.height : 1080, modelData.fps !== undefined ? modelData.fps : 60, modelData.totalFrames !== undefined ? modelData.totalFrames : 300, modelData.gridMode || "Auto", modelData.gridBpm !== undefined ? modelData.gridBpm : 120, modelData.gridOffset !== undefined ? modelData.gridOffset : 0, modelData.gridInterval !== undefined ? modelData.gridInterval : 10, modelData.gridSubdivision !== undefined ? modelData.gridSubdivision : 4, modelData.enableSnap !== undefined ? modelData.enableSnap : true, modelData.magneticSnapRange !== undefined ? modelData.magneticSnapRange : 10);

                                }
                            }

                            contentItem: RowLayout {
                                spacing: 4

                                Text {
                                    text: modelData.name
                                    font: tabBtn.font
                                    color: palette.text
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                    Layout.maximumWidth: 200
                                }

                                Button {
                                    flat: true
                                    visible: modelData.id !== 0
                                    hoverEnabled: true
                                    Layout.preferredWidth: 20
                                    Layout.preferredHeight: 20
                                    onClicked: {
                                        if (Workspace.currentTimeline)
                                            Workspace.currentTimeline.removeScene(modelData.id);

                                    }

                                    contentItem: Common.AviQtlIcon {
                                        iconName: "close_line"
                                        size: 14
                                        color: parent.hovered ? parent.palette.highlight : parent.palette.text
                                    }

                                }

                            }

                        }

                    }

                }

            }

            // シーン追加ボタン
            Button {
                flat: true
                Layout.preferredWidth: 40
                hoverEnabled: true
                Layout.fillHeight: true
                onClicked: {
                    var win = WindowManager.getWindow("sceneSettings");
                    if (win)
                        win.openForCreate(qsTr("シーン %1").arg(sceneRepeater.count + 1));

                }

                contentItem: Common.AviQtlIcon {
                    iconName: "add_line"
                    size: 20
                    color: parent.hovered ? parent.palette.highlight : parent.palette.text
                }

            }

        }

        // 2. 定規
        Ruler {
            targetFlickable: timelineView.flickable
            rulerHeight: timelineWindow.rulerHeight
            timeWidth: timelineWindow.headerWidth
            fps: Workspace.currentTimeline && Workspace.currentTimeline.project ? Workspace.currentTimeline.project.fps : 60
        }

        // 3. メインエリア
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            LayerHeader {
                id: layerHeader

                headerWidth: timelineWindow.headerWidth
                layerHeight: timelineWindow.layerHeight
                layerCount: timelineWindow.layerCount
                syncFlickable: timelineView.flickable
                onLayerVisibilityChanged: (layer, visible) => {
                    var newState = Object.assign({
                    }, globalLayerStates);
                    var oldState = newState[layer] || {
                        "visible": true,
                        "locked": false
                    };
                    newState[layer] = Object.assign({
                    }, oldState, {
                        "visible": visible
                    });
                    globalLayerStates = newState;
                }
                onLayerLockChanged: (layer, locked) => {
                    var newState = Object.assign({
                    }, globalLayerStates);
                    var oldState = newState[layer] || {
                        "visible": true,
                        "locked": false
                    };
                    newState[layer] = Object.assign({
                    }, oldState, {
                        "locked": locked
                    });
                    globalLayerStates = newState;
                }
            }

            TimelineView {
                id: timelineView

                Layout.fillWidth: true
                Layout.fillHeight: true
                layerHeight: timelineWindow.layerHeight
                layerCount: timelineWindow.layerCount
                clipResizeHandleWidth: timelineWindow.clipResizeHandleWidth
                getLayerLocked: (layer) => {
                    return layerHeader.getLayerLocked(layer);
                }
            }

        }

    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.delete"]) || "Delete"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.deleteSelectedClips()
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.split"]) || "S"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.splitSelectedClips(Workspace.currentTimeline.cursorFrame)
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.copy"]) || "Ctrl+C"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.copySelectedClips()
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.cut"]) || "Ctrl+X"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.cutSelectedClips()
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.paste"]) || "Ctrl+V"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.pasteClip(Workspace.currentTimeline.cursorFrame, Workspace.currentTimeline.selectedLayer)
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.duplicate"]) || "Ctrl+D"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: {
            Workspace.currentTimeline.copySelectedClips();
            Workspace.currentTimeline.pasteClip(Workspace.currentTimeline.cursorFrame, Workspace.currentTimeline.selectedLayer);
        }
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.moveUp"]) || "Alt+Up"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.moveSelectedClips(-1, 0)
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.moveDown"]) || "Alt+Down"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.moveSelectedClips(1, 0)
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.nudgeLeft"]) || "Alt+Left"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.moveSelectedClips(0, -1)
    }

    Shortcut {
        sequence: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.nudgeRight"]) || "Alt+Right"
        context: Qt.WindowShortcut
        enabled: !_isInputFocused && Workspace.currentTimeline
        onActivated: Workspace.currentTimeline.moveSelectedClips(0, 1)
    }

}
