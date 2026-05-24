import "../common" as Common
import "../common/Logger.js" as Logger
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: timelineViewRoot

    property alias flickable: timelineFlickable
    property alias contentX: timelineFlickable.contentX
    property alias contentY: timelineFlickable.contentY
    property int layerHeight: 30
    property int layerCount: 128
    property int clipResizeHandleWidth: 6
    property var getLayerLocked: function(layer) {
        return false;
    }
    property int contextClickFrame: 0
    property int contextClickLayer: 0
    property bool boxSelecting: false
    property point boxSelectionStart: Qt.point(0, 0)
    property point boxSelectionCurrent: Qt.point(0, 0)
    property real boxSelectionThreshold: 6
    property bool boxSelectionAdditive: false
    property int activeDragDeltaFrame: 0
    property bool autoScrollSuspended: false
    property int activeDragDeltaLayer: 0
    property bool isDraggingMulti: false
    property int selectionMinFrame: 0
    property var selectionVisualLatchIds: []
    property int selectionMinLayer: 0
    property int selectionMaxLayer: 0
    property bool dragAutoScrollActive: false
    property point dragViewportPos: Qt.point(-1, -1)
    property real dragScrollEdge: 48
    property real dragScrollStep: 24
    property var dragAutoScrollCallback: null
    property var currentSceneData: {
        if (!Workspace.currentTimeline || !Workspace.currentTimeline.scenes)
            return null;

        for (var i = 0; i < Workspace.currentTimeline.scenes.length; i++) {
            if (Workspace.currentTimeline.scenes[i].id === Workspace.currentTimeline.currentSceneId)
                return Workspace.currentTimeline.scenes[i];

        }
        return null;
    }
    property bool enableSnap: currentSceneData && currentSceneData.enableSnap !== undefined ? currentSceneData.enableSnap : (SettingsManager.settings ? SettingsManager.settings.enableSnap : true)
    property int magneticSnapRange: currentSceneData && currentSceneData.magneticSnapRange !== undefined ? currentSceneData.magneticSnapRange : (SettingsManager.settings ? SettingsManager.value("magneticSnapRange", 10) : 10)
    property int tailPaddingFrames: 120
    property var gridSettings: {
        if (currentSceneData)
            return {
            "mode": currentSceneData.gridMode || "Auto",
            "bpm": currentSceneData.gridBpm || 120,
            "offset": currentSceneData.gridOffset || 0,
            "interval": currentSceneData.gridInterval || 10,
            "subdivision": currentSceneData.gridSubdivision || 4
        };

        return {
            "mode": "Auto",
            "bpm": 120,
            "offset": 0,
            "interval": 10,
            "subdivision": 4
        };
    }
    readonly property int maxClipEndFrame: (Workspace.currentTimeline && Workspace.currentTimeline.timelineDuration > 0) ? Workspace.currentTimeline.timelineDuration : 0
    readonly property int timelineLengthFrames: Math.max(100, maxClipEndFrame + tailPaddingFrames)

    function beginDragAutoScroll(callback) {
        dragAutoScrollCallback = callback;
        dragAutoScrollActive = true;
    }

    function updateDragAutoScroll(posInViewport) {
        dragViewportPos = posInViewport;
    }

    function endDragAutoScroll() {
        dragAutoScrollActive = false;
        dragAutoScrollCallback = null;
    }

    function syncBoxSelectionPreview() {
        if (!Workspace.currentTimeline)
            return ;

        var scale = Workspace.currentTimeline.timelineScale;
        var f1 = Math.floor(boxSelectionStart.x / scale);
        var f2 = Math.ceil(boxSelectionCurrent.x / scale);
        var l1 = Math.floor(boxSelectionStart.y / layerHeight);
        var l2 = Math.floor(boxSelectionCurrent.y / layerHeight);
        Workspace.currentTimeline.updateSelectionPreview(f1, f2, l1, l2, boxSelectionAdditive);
    }

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function getGridInterval() {
        if (!Workspace.currentTimeline)
            return 1;

        var scale = Workspace.currentTimeline.timelineScale;
        var projectFps = (Workspace.currentTimeline.project && Workspace.currentTimeline.project.fps) ? Workspace.currentTimeline.project.fps : 60;
        if (gridSettings.mode === "BPM") {
            var beatFrames = projectFps / (gridSettings.bpm / 60);
            var bpmDiv = scale > 3 ? 4 : scale > 1.5 ? 2 : 1;
            return beatFrames / bpmDiv;
        }
        if (gridSettings.mode === "Frame")
            return gridSettings.interval;

        // Auto
        if (scale < 0.5)
            return Math.ceil(projectFps);

        if (scale < 1.5)
            return 10;

        if (scale < 3)
            return 5;

        return 1;
    }

    function snapFrame(frame, ignoreSnap) {
        if (!enableSnap || ignoreSnap)
            return Math.max(0, Math.round(frame));

        // グリッド無視時は整数丸めのみ
        var step = getGridInterval();
        var offset = (gridSettings.mode === "BPM" && Workspace.currentTimeline && Workspace.currentTimeline.project) ? gridSettings.offset * Workspace.currentTimeline.project.fps : 0;
        return Math.max(0, Math.round((Math.round((frame - offset) / step) * step) + offset));
    }

    clip: true
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOn
    ScrollBar.vertical.policy: ScrollBar.AlwaysOn

    Flickable {
        // unified loop handles viewport updates now
        id: timelineFlickable

        clip: true
        contentWidth: Math.max(width, timelineLengthFrames * (Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1))
        contentHeight: layerCount * layerHeight
        interactive: true
        onMovementStarted: timelineViewRoot.autoScrollSuspended = true

        Timer {
            id: renderTimer

            interval: 16
            repeat: true
            running: true // Unified render loop
            onTriggered: {
                if (!Workspace.currentTimeline)
                    return ;

                // 1. Viewport sync
                if (typeof Workspace.currentTimeline.updateViewport === "function")
                    Workspace.currentTimeline.updateViewport(timelineFlickable.contentX, timelineFlickable.contentY);

                if (timelineViewRoot.ScrollBar.horizontal && timelineViewRoot.ScrollBar.horizontal.pressed)
                    timelineViewRoot.autoScrollSuspended = true;

                if (timelineViewRoot.ScrollBar.vertical && timelineViewRoot.ScrollBar.vertical.pressed)
                    timelineViewRoot.autoScrollSuspended = true;

                // 2. Playhead auto-scroll (Page turn)
                if (Workspace.currentTimeline.transport && Workspace.currentTimeline.transport.isPlaying && !timelineViewRoot.autoScrollSuspended) {
                    let viewportWidth = timelineFlickable.width;
                    let playheadX = Workspace.currentTimeline.transport.currentFrame * Workspace.currentTimeline.timelineScale;
                    let left = timelineFlickable.contentX;
                    let right = left + viewportWidth;
                    let margin = 24;
                    if (playheadX < left || playheadX >= right - margin) {
                        let nextPage = Math.floor(playheadX / Math.max(1, viewportWidth));
                        let maxX = Math.max(0, timelineFlickable.contentWidth - viewportWidth);
                        timelineFlickable.contentX = clamp(nextPage * viewportWidth, 0, maxX);
                    }
                }
                // 3. Drag auto-scroll
                if (timelineViewRoot.dragAutoScrollActive) {
                    let dx = 0;
                    let dy = 0;
                    let edge = timelineViewRoot.dragScrollEdge;
                    let step = timelineViewRoot.dragScrollStep;
                    if (timelineViewRoot.dragViewportPos.x < edge)
                        dx = -step;
                    else if (timelineViewRoot.dragViewportPos.x > timelineFlickable.width - edge)
                        dx = step;
                    if (timelineViewRoot.dragViewportPos.y < edge)
                        dy = -step;
                    else if (timelineViewRoot.dragViewportPos.y > timelineFlickable.height - edge)
                        dy = step;
                    if (dx !== 0 || dy !== 0) {
                        let maxX = Math.max(0, timelineFlickable.contentWidth - timelineFlickable.width);
                        let maxY = Math.max(0, timelineFlickable.contentHeight - timelineFlickable.height);
                        timelineFlickable.contentX = clamp(timelineFlickable.contentX + dx, 0, maxX);
                        timelineFlickable.contentY = clamp(timelineFlickable.contentY + dy, 0, maxY);
                        if (timelineViewRoot.dragAutoScrollCallback)
                            timelineViewRoot.dragAutoScrollCallback();

                    }
                }
            }
        }

        Connections {
            function onTimelineScaleChanged() {
            }

            target: Workspace.currentTimeline ?? null
        }

        Connections {
            function onIsPlayingChanged() {
                if (Workspace.currentTimeline.transport.isPlaying)
                    timelineViewRoot.autoScrollSuspended = false;

            }

            target: Workspace.currentTimeline && Workspace.currentTimeline.transport ? Workspace.currentTimeline.transport : null
        }

        // 選択レイヤーの背景ハイライト
        Rectangle {
            visible: Workspace.currentTimeline !== null
            x: 0
            y: (Workspace.currentTimeline ? Workspace.currentTimeline.selectedLayer : 0) * layerHeight
            width: timelineFlickable.contentWidth
            height: layerHeight
            color: palette.highlight
            opacity: 0.08
            z: -2
        }

        // 編集カーソル（マウス追従ガイド）
        Rectangle {
            id: editCursorLine

            visible: Workspace.currentTimeline !== null
            x: (Workspace.currentTimeline ? Workspace.currentTimeline.cursorFrame : 0) * (Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1)
            y: 0
            width: 1
            height: timelineFlickable.contentHeight
            color: palette.highlight
            opacity: 0.5
            z: 90
        }

        Item {
            // 描画位置をピクセルにスナップさせてサブピクセル描画によるゴミを防ぐ
            x: Math.floor(timelineFlickable.contentX)
            y: Math.floor(timelineFlickable.contentY)
            width: timelineFlickable.width
            height: timelineFlickable.height
            z: -1

            TimelineGrid {
                id: timelineGrid

                anchors.fill: parent
                width: timelineFlickable.width
                height: timelineFlickable.height
                contentX: timelineFlickable.contentX
                contentY: timelineFlickable.contentY
                gridInterval: timelineViewRoot.getGridInterval()
                layerCount: timelineViewRoot.layerCount
                layerHeight: timelineViewRoot.layerHeight
                gridSettings: timelineViewRoot.gridSettings
            }

        }

        Repeater {
            model: Workspace.currentTimeline ? Workspace.currentTimeline.clips : []

            delegate: ClipItem {
                layerHeight: timelineViewRoot.layerHeight
                layerCount: timelineViewRoot.layerCount
                clipResizeHandleWidth: timelineViewRoot.clipResizeHandleWidth
                forceVisualSelection: false
                forcedSelectedIds: []
                flickableContentItem: timelineFlickable.contentItem
                snapFrameFunc: timelineViewRoot.snapFrame
                onClipMoved: (clipId, deltaLayer, deltaStart, unused) => {
                    if (Workspace.currentTimeline) {
                        var selectedIds = Workspace.currentTimeline.selection ? Workspace.currentTimeline.selection.selectedClipIds : [];
                        if (selectedIds.includes(clipId)) {
                            var moves = [];
                            for (var i = 0; i < Workspace.currentTimeline.clips.length; i++) {
                                var c = Workspace.currentTimeline.clips[i];
                                if (selectedIds.includes(c.id)) {
                                    var newL = Math.round(Number(c.layer) + Number(deltaLayer));
                                    var newF = Math.round(Number(c.startFrame) + Number(deltaStart));
                                    if (newL < 0)
                                        newL = 0;

                                    if (newL >= timelineViewRoot.layerCount)
                                        newL = timelineViewRoot.layerCount - 1;

                                    if (newF < 0)
                                        newF = 0;

                                    moves.push({
                                        "id": Number(c.id),
                                        "layer": newL,
                                        "startFrame": newF,
                                        "duration": Number(c.durationFrames)
                                    });
                                }
                            }
                            Workspace.currentTimeline.applyClipBatchMove(moves);
                        } else {
                            // Should not happen with new UX fix, but fallback
                            var c = Workspace.currentTimeline.clips.find((c) => {
                                return c.id === clipId;
                            });
                            if (c)
                                Workspace.currentTimeline.updateClip(clipId, Math.max(0, c.layer + deltaLayer), Math.max(0, c.startFrame + deltaStart), c.durationFrames);

                        }
                    }
                }
                onClipResized: (clipId, deltaStart, deltaDuration, unused) => {
                    if (Workspace.currentTimeline) {
                        if (Workspace.currentTimeline && Workspace.currentTimeline.selection && Workspace.currentTimeline.selection.selectedClipIds.includes(clipId)) {
                            Workspace.currentTimeline.resizeSelectedClips(deltaStart, deltaDuration);
                        } else {
                            var c = Workspace.currentTimeline.clips.find((c) => {
                                return c.id === clipId;
                            });
                            if (c)
                                Workspace.currentTimeline.updateClip(clipId, c.layer, Math.max(0, c.startFrame + deltaStart), Math.max(1, c.durationFrames + deltaDuration));

                        }
                    }
                }
                onClipDoubleClicked: (clipId) => {
                    if (WindowManager)
                        WindowManager.raiseWindow("objectSettings");

                }
            }

        }

        Rectangle {
            id: playhead

            x: (Workspace.currentTimeline && Workspace.currentTimeline.transport ? Workspace.currentTimeline.transport.currentFrame : 0) * (Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1)
            y: 0
            width: 2
            height: parent.height
            color: palette.highlight
            z: 100
        }

        MouseArea {
            anchors.fill: parent
            z: -1
            acceptedButtons: Qt.LeftButton
            cursorShape: Qt.ArrowCursor
            preventStealing: true
            hoverEnabled: true
            onPositionChanged: (mouse) => {
                if (pressed && Workspace.currentTimeline)
                    Workspace.currentTimeline.cursorFrame = timelineViewRoot.snapFrame(mouse.x / Workspace.currentTimeline.timelineScale, (mouse.modifiers & Qt.ShiftModifier));

            }
            onPressed: (mouse) => {
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.cursorFrame = timelineViewRoot.snapFrame(mouse.x / Workspace.currentTimeline.timelineScale, (mouse.modifiers & Qt.ShiftModifier));

                var l = Math.floor(mouse.y / layerHeight);
                if (Workspace.currentTimeline && l >= 0 && l < layerCount) {
                    Workspace.currentTimeline.selectedLayer = l;
                    Workspace.currentTimeline.clearSelectionPreview();
                    Workspace.currentTimeline.applySelectionIds([]);
                }
            }
        }

        MouseArea {
            anchors.fill: parent
            z: -1
            acceptedButtons: Qt.RightButton
            preventStealing: true
            cursorShape: boxSelecting ? Qt.CrossCursor : Qt.ArrowCursor
            onPressed: (mouse) => {
                boxSelecting = false;
                boxSelectionStart = mapToItem(timelineFlickable.contentItem, mouse.x, mouse.y);
                boxSelectionCurrent = boxSelectionStart;
                boxSelectionAdditive = !!(mouse.modifiers & Qt.ControlModifier);
                if (Workspace.currentTimeline)
                    Workspace.currentTimeline.clearSelectionPreview();

                var l = Math.floor(mouse.y / layerHeight);
                if (Workspace.currentTimeline && l >= 0 && l < layerCount)
                    Workspace.currentTimeline.selectedLayer = l;

            }
            onPositionChanged: (mouse) => {
                boxSelectionCurrent = mapToItem(timelineFlickable.contentItem, mouse.x, mouse.y);
                if (Math.abs(boxSelectionCurrent.x - boxSelectionStart.x) >= boxSelectionThreshold || Math.abs(boxSelectionCurrent.y - boxSelectionStart.y) >= boxSelectionThreshold) {
                    boxSelecting = true;
                    syncBoxSelectionPreview();
                }
            }
            onReleased: (mouse) => {
                boxSelectionCurrent = mapToItem(timelineFlickable.contentItem, mouse.x, mouse.y);
                if (!boxSelecting) {
                    var scale = Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1;
                    var frame = timelineViewRoot.snapFrame(boxSelectionCurrent.x / scale);
                    var layer = Math.floor(boxSelectionCurrent.y / layerHeight);
                    var clickedClipId = -1;
                    if (Workspace.currentTimeline && Workspace.currentTimeline.clips) {
                        for (var i = Workspace.currentTimeline.clips.length - 1; i >= 0; i--) {
                            var c = Workspace.currentTimeline.clips[i];
                            if (c.layer === layer && frame >= c.startFrame && frame < c.startFrame + c.durationFrames) {
                                clickedClipId = c.id;
                                break;
                            }
                        }
                    }
                    if (clickedClipId >= 0 && Workspace.currentTimeline && Workspace.currentTimeline.selection && !Workspace.currentTimeline.selection.selectedClipIds.includes(clickedClipId))
                        Workspace.currentTimeline.applySelectionIds([clickedClipId]);

                    contextMenu.openAt(mouse.x, mouse.y, clickedClipId >= 0 ? "clip" : "timeline", frame, layer, clickedClipId);
                    return ;
                }
                if (Workspace.currentTimeline) {
                    syncBoxSelectionPreview(); // 確定直前に最終座標で同期
                    Workspace.currentTimeline.finalizeSelectionPreview();
                }
                boxSelecting = false;
            }
        }

        Rectangle {
            visible: boxSelecting
            z: 1000
            color: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.2)
            border.color: palette.highlight
            border.width: 1
            x: Math.min(boxSelectionStart.x, boxSelectionCurrent.x)
            y: Math.min(boxSelectionStart.y, boxSelectionCurrent.y)
            width: Math.abs(boxSelectionCurrent.x - boxSelectionStart.x)
            height: Math.abs(boxSelectionCurrent.y - boxSelectionStart.y)
        }

        MouseArea {
            anchors.fill: parent
            z: -1
            acceptedButtons: Qt.NoButton
            onWheel: (wheel) => {
                timelineViewRoot.autoScrollSuspended = true;
                var dy = (wheel.pixelDelta && wheel.pixelDelta.y !== 0) ? wheel.pixelDelta.y * 10 : wheel.angleDelta.y;
                var dx = (wheel.pixelDelta && wheel.pixelDelta.x !== 0) ? wheel.pixelDelta.x * 10 : wheel.angleDelta.x;
                if (wheel.modifiers & Qt.AltModifier || wheel.modifiers & Qt.ControlModifier) {
                    // Zoom
                    if (Workspace.currentTimeline) {
                        var step = SettingsManager ? SettingsManager.value("timelineZoomStep", 10) : 10;
                        var minZ = SettingsManager ? SettingsManager.value("timelineZoomMin", 10) : 10;
                        var maxZ = SettingsManager ? SettingsManager.value("timelineZoomMax", 400) : 400;
                        var direction = (Math.abs(dy) > Math.abs(dx) ? dy : dx) > 0 ? 1 : -1;
                        var newScale = Workspace.currentTimeline.timelineScale + (direction * step / 100);
                        newScale = clamp(newScale, minZ / 100, maxZ / 100);
                        // Zoom keeping the mouse position stationary if possible
                        var contentX = timelineFlickable.contentX;
                        var mouseX = wheel.x;
                        var frameAtMouse = (contentX + mouseX) / Workspace.currentTimeline.timelineScale;
                        Workspace.currentTimeline.timelineScale = newScale;
                        // Adjust scroll to keep frameAtMouse at mouseX
                        var newContentX = frameAtMouse * newScale - mouseX;
                        var maxX = Math.max(0, timelineFlickable.contentWidth - timelineFlickable.width);
                        timelineFlickable.contentX = clamp(newContentX, 0, maxX);
                    }
                } else if (wheel.modifiers & Qt.ShiftModifier) {
                    // Vertical Scroll
                    var maxY = Math.max(0, timelineFlickable.contentHeight - timelineFlickable.height);
                    timelineFlickable.contentY = clamp(timelineFlickable.contentY - dy, 0, maxY);
                } else {
                    // Horizontal Scroll
                    var delta = (Math.abs(dx) > Math.abs(dy)) ? dx : dy;
                    var maxX = Math.max(0, timelineFlickable.contentWidth - timelineFlickable.width);
                    timelineFlickable.contentX = clamp(timelineFlickable.contentX - delta, 0, maxX);
                }
                wheel.accepted = true;
            }
        }

    }

    Menu {
        // プラットフォーム固有の項目など、破棄不可能なオブジェクトはスキップする

        id: contextMenu

        property string targetType: ""
        property int targetClipId: -1

        function openAt(x, y, type, frame, layer, clipId) {
            targetType = type;
            targetClipId = clipId;
            contextClickFrame = frame;
            contextClickLayer = layer;
            rebuildMenu();
            popup();
        }

        function createMenuItem(label, cmd, icon) {
            var item = menuItemComp.createObject(timelineViewRoot, {
                "text": label,
                "iconName": icon || ""
            });
            item.triggered.connect(() => {
                return handleCommand(cmd);
            });
            return item;
        }

        function createSubMenu(label) {
            return subMenuComp.createObject(timelineViewRoot, {
                "title": label
            });
        }

        function addSeparator() {
            contextMenu.addItem(menuSeparatorComp.createObject(timelineViewRoot));
        }

        function shouldApplyToSelection() {
            if (!Workspace.currentTimeline || !Workspace.currentTimeline.selection || targetClipId < 0)
                return false;

            var ids = Workspace.currentTimeline.selection.selectedClipIds;
            if (!ids || ids.length <= 1)
                return false;

            for (var i = 0; i < ids.length; i++) {
                if (ids[i] === targetClipId)
                    return true;

            }
            return false;
        }

        function handleCommand(cmd) {
            if (!Workspace.currentTimeline)
                return ;

            if (cmd.startsWith("add.")) {
                Workspace.currentTimeline.createObject(cmd.substring(4), Workspace.currentTimeline.cursorFrame, Workspace.currentTimeline.selectedLayer);
                return ;
            }
            switch (cmd) {
            case "edit.undo":
                Workspace.currentTimeline.undo();
                break;
            case "edit.redo":
                Workspace.currentTimeline.redo();
                break;
            case "clip.delete":
                if (shouldApplyToSelection())
                    Workspace.currentTimeline.deleteSelectedClips();
                else
                    Workspace.currentTimeline.deleteClip(targetClipId);
                break;
            case "clip.split":
                Workspace.currentTimeline.splitClip(targetClipId, Workspace.currentTimeline.cursorFrame);
                break;
            case "clip.duplicate":
                Workspace.currentTimeline.copySelectedClips();
                Workspace.currentTimeline.pasteClip(Workspace.currentTimeline.cursorFrame, Workspace.currentTimeline.selectedLayer);
                break;
            case "clip.cut":
                if (shouldApplyToSelection())
                    Workspace.currentTimeline.cutSelectedClips();
                else
                    Workspace.currentTimeline.cutClip(targetClipId);
                break;
            case "clip.copy":
                if (shouldApplyToSelection())
                    Workspace.currentTimeline.copySelectedClips();
                else
                    Workspace.currentTimeline.copyClip(targetClipId);
                break;
            case "edit.paste":
                Workspace.currentTimeline.pasteClip(Workspace.currentTimeline.cursorFrame, Workspace.currentTimeline.selectedLayer);
                break;
            case "view.scenesettings":
                var win = WindowManager.getWindow("sceneSettings");
                if (win && Workspace.currentTimeline && Workspace.currentTimeline.scenes) {
                    var scenes = Workspace.currentTimeline.scenes;
                    var curId = Workspace.currentTimeline.currentSceneId;
                    var curScene = null;
                    for (var i = 0; i < scenes.length; i++) {
                        if (scenes[i].id === curId) {
                            curScene = scenes[i];
                            break;
                        }
                    }
                    if (curScene) {
                        win.openForScene(curScene.id, curScene.name, curScene.width !== undefined ? curScene.width : 1920, curScene.height !== undefined ? curScene.height : 1080, curScene.fps !== undefined ? curScene.fps : 60, curScene.totalFrames !== undefined ? curScene.totalFrames : 300, curScene.gridMode || "Auto", curScene.gridBpm !== undefined ? curScene.gridBpm : 120, curScene.gridOffset !== undefined ? curScene.gridOffset : 0, curScene.gridInterval !== undefined ? curScene.gridInterval : 10, curScene.gridSubdivision !== undefined ? curScene.gridSubdivision : 4, curScene.enableSnap !== undefined ? curScene.enableSnap : true, curScene.magneticSnapRange !== undefined ? curScene.magneticSnapRange : 10);
                    } else {
                        win.show();
                        win.raise();
                        win.requestActivate();
                    }
                } else if (win) {
                    win.show();
                    win.raise();
                    win.requestActivate();
                }
                break;
            case "view.projectsettings":
                if (WindowManager)
                    WindowManager.projectSettingsVisible = true;

                break;
            case "view.systemsettings":
                if (WindowManager)
                    WindowManager.systemSettingsVisible = true;

                break;
            default:
                // Unknown command: logged via debug channel instead of console.log
                // so it does not slow down the message handler in production builds.
                Logger.log("[TimelineView] Unknown command: " + cmd, Workspace.currentTimeline);
            }
        }

        function rebuildMenu() {
            function buildObjMenu(parentMenu, items) {
                for (var i = 0; i < items.length; ++i) {
                    var node = items[i];
                    if (node.isCategory) {
                        // addMenu(string) はネイティブメニューハンドルが未確定の場合に
                        // Qt内部でnullポインタ参照を起こすため、Component経由で生成する
                        var subMenu = subMenuComp.createObject(parentMenu, {
                            "title": node.title
                        });
                        buildObjMenu(subMenu, node.children);
                        parentMenu.addMenu(subMenu);
                    } else {
                        var objItem = menuItemComp.createObject(parentMenu, {
                            "text": node.name,
                            "iconName": "shape_line"
                        });
                        (function(id) {
                            objItem.triggered.connect(() => {
                                return handleCommand("add." + id);
                            });
                        })(node.id);
                        parentMenu.addItem(objItem);
                    }
                }
            }

            function buildEffectMenu(parentMenu, items) {
                for (var i = 0; i < items.length; ++i) {
                    var node = items[i];
                    if (node.isCategory) {
                        var subMenu = subMenuComp.createObject(parentMenu, {
                            "title": node.title
                        });
                        buildEffectMenu(subMenu, node.children);
                        parentMenu.addMenu(subMenu);
                    } else {
                        var effItem = menuItemComp.createObject(parentMenu, {
                            "text": node.name,
                            "iconName": "magic_line"
                        });
                        (function(id) {
                            effItem.triggered.connect(() => {
                                Workspace.currentTimeline.addEffect(targetClipId, id);
                            });
                        })(node.id);
                        parentMenu.addItem(effItem);
                    }
                }
            }

            function buildAudioPluginMenu(parentMenu) {
                var categories = Workspace.currentTimeline.getPluginCategories();
                for (var c = 0; c < categories.length; c++) {
                    var catName = categories[c];
                    var subMenu = subMenuComp.createObject(timelineViewRoot, {
                        "title": catName
                    });
                    var plugins = Workspace.currentTimeline.getPluginsByCategory(catName);
                    for (var p = 0; p < plugins.length; p++) {
                        (function(pluginData) {
                            var plugItem = menuItemComp.createObject(subMenu, {
                                "text": pluginData.name,
                                "iconName": "music_line"
                            });
                            plugItem.triggered.connect(() => {
                                Workspace.currentTimeline.addAudioPlugin(targetClipId, pluginData.id);
                            });
                            subMenu.addItem(plugItem);
                        })(plugins[p]);
                    }
                    parentMenu.addMenu(subMenu);
                }
            }

            while (contextMenu.count > 0) {
                var it = contextMenu.takeItem(0);
                if (it) {
                    try {
                        it.destroy();
                    } catch (e) {
                    }
                }
            }
            if (targetType === "timeline") {
                var objectMenu = subMenuComp.createObject(contextMenu, {
                    "title": qsTr("オブジェクトを追加")
                });
                objectMenu.aboutToShow.connect(function() {
                    // すでに構築済み（ホバーし直し等）なら何もしない。
                    // これにより項目が減っていく奇妙なバグを回避する。
                    if (objectMenu.count > 0)
                        return ;

                    var objects = Workspace.currentTimeline.getAvailableObjects();
                    buildObjMenu(objectMenu, objects);
                });
                contextMenu.addMenu(objectMenu);
                addSeparator();
                contextMenu.addItem(createMenuItem(qsTr("元に戻す"), "edit.undo", "arrow_go_back_line"));
                contextMenu.addItem(createMenuItem(qsTr("やり直す"), "edit.redo", "arrow_go_forward_line"));
                contextMenu.addItem(createMenuItem(qsTr("貼り付け"), "edit.paste", "clipboard_line"));
                addSeparator();
                contextMenu.addItem(createMenuItem(qsTr("シーン設定..."), "view.scenesettings", "settings_6_line"));
                contextMenu.addItem(createMenuItem(qsTr("プロジェクト設定..."), "view.projectsettings", "settings_4_line"));
                contextMenu.addItem(createMenuItem(qsTr("環境設定..."), "view.systemsettings", "settings_3_line"));
            } else if (targetType === "clip") {
                contextMenu.addItem(createMenuItem(qsTr("削除"), "clip.delete", "delete_bin_line"));
                contextMenu.addItem(createMenuItem(qsTr("分割"), "clip.split", "scissors_cut_line"));
                contextMenu.addItem(createMenuItem(qsTr("複製"), "clip.duplicate", "file_copy_2_line"));
                addSeparator();
                contextMenu.addItem(createMenuItem(qsTr("切り取り"), "clip.cut", "scissors_line"));
                contextMenu.addItem(createMenuItem(qsTr("コピー"), "clip.copy", "file_copy_line"));
                addSeparator();
                var addEffSub = subMenuComp.createObject(contextMenu, {
                    "title": qsTr("エフェクトを追加")
                });
                addEffSub.aboutToShow.connect(function() {
                    if (addEffSub.count > 0)
                        return ;

                    if (Workspace.currentTimeline.isAudioClip(targetClipId)) {
                        buildAudioPluginMenu(addEffSub);
                    } else {
                        var effects = Workspace.currentTimeline.getAvailableEffects();
                        buildEffectMenu(addEffSub, effects);
                    }
                });
                contextMenu.addMenu(addEffSub);
            }
        }

        Component {
            id: menuItemComp

            Common.IconMenuItem {
            }

        }

        Component {
            id: subMenuComp

            Menu {
            }

        }

        Component {
            id: menuSeparatorComp

            MenuSeparator {
            }

        }

    }

}
