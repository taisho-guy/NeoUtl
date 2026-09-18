import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common
import "timeline" 
Common.AviQtlWindow {
    
    id: timelineWindow

        property var settings: SettingsManager.settings
    readonly
    property int layerCount: settings.timelineMaxLayers || 128
    readonly property int layerHeight: settings.timelineTrackHeight || 30
    readonly
    property int rulerHeight: settings.timelineRulerHeight || 32
    readonly property int headerWidth: settings.timelineLayerHeaderWidth || 60
    readonly
    property int clipResizeHandleWidth: settings.timelineClipResizeHandleWidth || 10
    readonly property int sceneTabHeight: settings.timelineHeaderHeight || 28
    property var globalLayerStates: ({
    })
    readonly property bool _isInputFocused: {
        var item = Qt.application.focusItem;
        if (!item)
            return false;

                return item.hasOwnProperty("echoMode") || (item.hasOwnProperty("selectionStart") && item.readOnly === false);
    }

    function getLayerVisible(layer) {
        var state = globalLayerStates[layer];
        return state ? state.visible : true;
    }

    function currentSceneData() {
        if (!Workspace.currentTimeline || !Workspace.currentTimeline.scenes)
            return null;

        for (var i = 0; i < Workspace.currentTimeline.scenes.length; i++) {
            if (Workspace.currentTimeline.scenes[i].id === Workspace.currentTimeline.currentSceneId)
                return Workspace.currentTimeline.scenes[i];

        }
        return null;
    }

    function syncGlobalLayerStates() {
        if (!Workspace.currentTimeline)
            return ;

        var next = ({
        });
        for (var i = 0; i < layerCount; i++) {
            next[i] = {
                "visible": !Workspace.currentTimeline.isLayerHidden(i),
                "locked": Workspace.currentTimeline.isLayerLocked(i)
            };
        }
        globalLayerStates = next;
    }

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function zoomPercentToScale(percent) {
        if (percent <= 100)
            return percent / 100;

        return 1 + ((percent - 100) * 9 / 300);
    }

    function scaleToZoomPercent(scale) {
        if (scale <= 1)
            return scale * 100;

        return 100 + ((scale - 1) * 300 / 9);
    }

    function setTimelineZoomPercent(percent, anchorMode) {
        if (!Workspace.currentTimeline)
            return ;

        var minZ = SettingsManager ? SettingsManager.value("timelineZoomMin", 10) : 10;
        var maxZ = SettingsManager ? SettingsManager.value("timelineZoomMax", 400) : 400;
        var nextPercent = clamp(percent, minZ, maxZ);
        var flick = timelineView.flickable;
        var oldScale = Workspace.currentTimeline.timelineScale;
        var anchorX = anchorMode === "center" && flick ? flick.width / 2 : 0;
        var anchorFrame = flick && oldScale > 0 ? (flick.contentX + anchorX) / oldScale : 0;
        Workspace.currentTimeline.timelineScale = zoomPercentToScale(nextPercent);
        if (flick) {
            var maxX = Math.max(0, flick.contentWidth - flick.width);
            flick.contentX = clamp(anchorFrame * Workspace.currentTimeline.timelineScale - anchorX, 0, maxX);
        }
    }

    title: qsTr("タイムライン")
    objectName: "timelineWindow"
    width: 1280
    height: 300
    Component.onCompleted: syncGlobalLayerStates()

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: sceneTabHeight
            Layout.minimumHeight: sceneTabHeight
            Layout.maximumHeight: sceneTabHeight
            spacing: 0
            z: 1

            ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: sceneTabHeight
                Layout.minimumHeight: sceneTabHeight
                Layout.maximumHeight: sceneTabHeight
                implicitHeight: sceneTabHeight
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                TabBar {
                    id: sceneTabBar

                                        width: Math.max(parent.width, contentWidth)
                    height: sceneTabHeight
                    implicitHeight: sceneTabHeight

                    Repeater {
                        id: sceneRepeater

                        model: Workspace.currentTimeline ? Workspace.currentTimeline.scenes : []

                        TabButton {
                            id: tabBtn

                            readonly property color _displayBgColor: checked ? palette.highlight : palette.window
                            readonly
    property color _contrastColor: {
                                var bg = _displayBgColor;
                                                                var luma = (0.299 * bg.r) + (0.587 * bg.g) + (0.114 * bg.b);
                                return luma > 0.6 ? "#000000" : "#ffffff";
                            }

                            implicitWidth: Math.max(100, contentItem.implicitWidth + leftPadding + rightPadding)
                            height: sceneTabHeight
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
                                    color: tabBtn._contrastColor
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
                                        color: parent.hovered ? parent.palette.highlight : tabBtn._contrastColor
                                    }

                                }

                            }

                            background: Rectangle {
                                color: tabBtn.checked ? palette.highlight : (tabBtn.hovered ? Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.16) : "transparent")
                                border.color: tabBtn.checked ? palette.highlight : palette.mid
                                border.width: tabBtn.checked ? 1 : 0
                            }

                        }

                    }

                }

            }

            Button {
                flat: true
                Layout.preferredWidth: 40
                Layout.preferredHeight: sceneTabHeight
                Layout.minimumHeight: sceneTabHeight
                Layout.maximumHeight: sceneTabHeight
                hoverEnabled: true
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

        Ruler {
            targetFlickable: timelineView.flickable
            rulerHeight: timelineWindow.rulerHeight
            timeWidth: timelineWindow.headerWidth
            fps: Workspace.currentTimeline && Workspace.currentTimeline.project ? Workspace.currentTimeline.project.fps : 60
            timelineDuration: timelineView.timelineLengthFrames
            Layout.preferredHeight: timelineWindow.rulerHeight
            Layout.minimumHeight: timelineWindow.rulerHeight
            Layout.maximumHeight: timelineWindow.rulerHeight
            onZoomRequested: (p) => {
                return setTimelineZoomPercent(p, "center");
            }
        }

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

    Connections {
        function onCurrentTimelineChanged() {
            timelineWindow.syncGlobalLayerStates();
                        if (Workspace.currentTimeline)
                timelineWindow.setTimelineZoomPercent(100, "center");

        }

        target: Workspace
    }

    Connections {
        function onClipsChanged() {
            timelineWindow.syncGlobalLayerStates();
        }

        function onCurrentSceneIdChanged() {
            timelineWindow.syncGlobalLayerStates();
        }

        function onScenesChanged() {
            timelineWindow.syncGlobalLayerStates();
        }

        target: Workspace.currentTimeline
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
