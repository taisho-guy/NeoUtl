import QtQuick
import QtQuick.Controls
import ".."

Item {
    id: timelineViewRoot

    property alias flickable: timelineFlickable
    property alias contentX: timelineFlickable.contentX
    property alias contentY: timelineFlickable.contentY
    property int layerHeight: 30
    property int layerCount: 128
    property int clipResizeHandleWidth: 6
    property real scale: 1
    property int projectFps: 30
    property int currentFrame: 0
    property bool isPlaying: false
    property var clips: []
    property int contextClickFrame: 0
    property int contextClickLayer: 0
    property bool boxSelecting: false
    property point boxSelectionStart: Qt.point(0, 0)
    property point boxSelectionCurrent: Qt.point(0, 0)
    property real boxSelectionThreshold: 6
    property bool boxSelectionAdditive: false

    property int activeDragDeltaFrame: 0
    property int activeDragDeltaLayer: 0
    property bool isDraggingMulti: false
    property bool autoScrollSuspended: false
    property bool dragAutoScrollActive: false
    property point dragViewportPos: Qt.point(-1, -1)
    property real dragScrollEdge: 48
    property real dragScrollStep: 24
    property var dragAutoScrollCallback: null
    property bool enableSnap: true
    property int magneticSnapRange: 5
    property var gridSettings: ({
        "mode": "Auto",
        "bpm": 120,
        "offset": 0,
        "interval": 10,
        "subdivision": 4
    })

    property int tailPaddingFrames: 120
    readonly
    property int maxClipEndFrame: {
        var maxEnd = 0;
        for (var i = 0; i < clips.length; i++) {
            var e = clips[i].startFrame + clips[i].durationFrames;
            if (e > maxEnd)
                maxEnd = e;

        }
        return maxEnd;
    }
    property int sceneTotalFrames: 0
    readonly
    property int timelineLengthFrames: Math.max(100, sceneTotalFrames,
        maxClipEndFrame + tailPaddingFrames)
    readonly property int scrollBarThickness: 14

    signal contextMenuRequested(int frame, int layer, int clipId)
    signal clipDoubleClicked(int clipId)

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

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function zoomPercentToScale(percent) {
        if (percent <= 100)
            return percent / 100;

        return 1 + ((percent - 100) * 9 / 300);
    }

    function scaleToZoomPercent(s) {
        if (s <= 1)
            return s * 100;

        return 100 + ((s - 1) * 300 / 9);
    }

    function getGridInterval() {
        if (gridSettings.mode === "BPM") {
            var beatFrames = projectFps / (gridSettings.bpm / 60);
            var bpmDiv = scale > 3 ? 4 : scale > 1.5 ? 2 : 1;
            return beatFrames / bpmDiv;
        }
        if (gridSettings.mode === "Frame")
            return gridSettings.interval;

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

        var step = getGridInterval();
        var offset = (gridSettings.mode === "BPM") ? gridSettings.offset * projectFps : 0;
        return Math.max(0, Math.round((Math.round((frame - offset) / step) * step) + offset));
    }

    function syncBoxSelectionPreview() {
        var f1 = Math.floor(boxSelectionStart.x / scale);
        var f2 = Math.ceil(boxSelectionCurrent.x / scale);
        var l1 = Math.floor(boxSelectionStart.y / layerHeight);
        var l2 = Math.floor(boxSelectionCurrent.y / layerHeight);
        TimelineBridge.selectInRect(Math.min(f1, f2) * scale, Math.min(l1, l2) * layerHeight,
            Math.max(f1, f2) * scale, Math.max(l1, l2) * layerHeight);
    }

    clip: true

    Flickable {
        id: timelineFlickable

        anchors.fill: parent
        clip: true
        contentWidth: timelineLengthFrames * timelineViewRoot.scale
        contentHeight: layerCount * layerHeight
        interactive: false

        Timer {
            id: renderTimer

            interval: 16
            repeat: true
            running: true
            onTriggered: {
                TimelineBridge.setViewport(timelineFlickable.contentX, timelineFlickable.contentY,
                    timelineFlickable.width, timelineFlickable.height);
                if (horizontalScrollBar.active
                    && (horizontalScrollBar.pressed || horizontalScrollBar.hovered))
                    timelineViewRoot.autoScrollSuspended = true;

                if (verticalScrollBar.active
                    && (verticalScrollBar.pressed || verticalScrollBar.hovered))
                    timelineViewRoot.autoScrollSuspended = true;

                if (timelineViewRoot.isPlaying && !timelineViewRoot.autoScrollSuspended) {
                    let viewportWidth = timelineFlickable.width;
                    let playheadX = timelineViewRoot.currentFrame * timelineViewRoot.scale;
                    let left = timelineFlickable.contentX;
                    let right = left + viewportWidth;
                    let margin = 24;
                    if (playheadX < left || playheadX >= right - margin) {
                        let nextPage = Math.floor(playheadX / Math.max(1, viewportWidth));
                        let maxX = Math.max(0, timelineFlickable.contentWidth - viewportWidth);
                        timelineFlickable.contentX = clamp(nextPage * viewportWidth, 0, maxX);
                    }
                }
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

        Item {
            x: Math.floor(timelineFlickable.contentX)
            y: Math.floor(timelineFlickable.contentY)
            width: timelineFlickable.width
            height: timelineFlickable.height
            z: -1

            TimelineGrid {
                id: timelineGrid

                anchors.fill: parent
                contentX: timelineFlickable.contentX
                contentY: timelineFlickable.contentY
                gridInterval: timelineViewRoot.getGridInterval()
                layerCount: timelineViewRoot.layerCount
                layerHeight: timelineViewRoot.layerHeight
                gridSettings: timelineViewRoot.gridSettings
                scale: timelineViewRoot.scale
                projectFps: timelineViewRoot.projectFps
            }
        }

        MouseArea {
            anchors.fill: parent
            z: -1
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            preventStealing: true
            hoverEnabled: true

            onPressed: (mouse) => {
                if (mouse.button === Qt.RightButton) {
                    timelineViewRoot.contextClickFrame = timelineViewRoot.snapFrame(
                        mouse.x / timelineViewRoot.scale, true);
                    timelineViewRoot.contextClickLayer = Math.floor(mouse.y / layerHeight);
                    timelineViewRoot.contextMenuRequested(timelineViewRoot.contextClickFrame,
                        timelineViewRoot.contextClickLayer, -1);
                    return ;
                }
                TimelineBridge.seek(timelineViewRoot.snapFrame(mouse.x / timelineViewRoot.scale,
                    (mouse.modifiers & Qt.ShiftModifier) !== 0));
                timelineViewRoot.boxSelectionStart = Qt.point(mouse.x, mouse.y);
                timelineViewRoot.boxSelectionCurrent = Qt.point(mouse.x, mouse.y);
                timelineViewRoot.boxSelectionAdditive = (mouse.modifiers & Qt.ControlModifier) !== 0;
                timelineViewRoot.boxSelecting = false;
            }

            onPositionChanged: (mouse) => {
                if (!pressed)
                    return ;

                timelineViewRoot.boxSelectionCurrent = Qt.point(mouse.x, mouse.y);
                if (!timelineViewRoot.boxSelecting) {
                    var moved = Math.abs(mouse.x - timelineViewRoot.boxSelectionStart.x)
                        + Math.abs(mouse.y - timelineViewRoot.boxSelectionStart.y);
                    if (moved < timelineViewRoot.boxSelectionThreshold) {
                        TimelineBridge.seek(timelineViewRoot.snapFrame(
                            mouse.x / timelineViewRoot.scale,
                            (mouse.modifiers & Qt.ShiftModifier) !== 0));
                        return ;
                    }
                    timelineViewRoot.boxSelecting = true;
                }
            }

            onReleased: {
                if (timelineViewRoot.boxSelecting) {
                    timelineViewRoot.syncBoxSelectionPreview();
                    timelineViewRoot.boxSelecting = false;
                } else if (!timelineViewRoot.boxSelectionAdditive) {
                    TimelineBridge.clearSelection();
                }
            }
        }

        Repeater {
            model: timelineViewRoot.clips

            delegate: ClipItem {
                layerHeight: timelineViewRoot.layerHeight
                layerCount: timelineViewRoot.layerCount
                clipResizeHandleWidth: timelineViewRoot.clipResizeHandleWidth
                flickableContentItem: timelineFlickable.contentItem
                snapFrameFunc: timelineViewRoot.snapFrame
                scale: timelineViewRoot.scale
                timelineViewRoot: timelineViewRoot
                onClipMoved: (clipId, deltaLayer, deltaStart, duration) => {
                    TimelineBridge.applyClipBatchMove(clipId, deltaLayer, deltaStart);
                }
                onClipResized: (clipId, deltaStart, deltaDuration, unused) => {
                    TimelineBridge.applyClipResize(clipId, deltaStart, deltaDuration);
                }
                onClipDoubleClicked: (clipId) => timelineViewRoot.clipDoubleClicked(clipId)
                onClipContextRequested: (clipId, frame, layer) => {
                    timelineViewRoot.contextMenuRequested(frame, layer, clipId);
                }
            }
        }

        Rectangle {
            id: boxSelectionRect

            visible: timelineViewRoot.boxSelecting
            color: Qt.rgba(0.4, 0.6, 1, 0.2)
            border.color: Qt.rgba(0.4, 0.6, 1, 0.9)
            border.width: 1
            x: Math.min(timelineViewRoot.boxSelectionStart.x, timelineViewRoot.boxSelectionCurrent.x)
            y: Math.min(timelineViewRoot.boxSelectionStart.y, timelineViewRoot.boxSelectionCurrent.y)
            width: Math.abs(timelineViewRoot.boxSelectionCurrent.x - timelineViewRoot.boxSelectionStart.x)
            height: Math.abs(timelineViewRoot.boxSelectionCurrent.y - timelineViewRoot.boxSelectionStart.y)
            z: 20
        }

        Rectangle {
            id: playhead

            x: timelineViewRoot.currentFrame * timelineViewRoot.scale
            y: timelineFlickable.contentY
            width: 2
            height: timelineFlickable.height
            color: Theme.playhead
            z: 30
        }

        ScrollBar.horizontal: ScrollBar {
            id: horizontalScrollBar

            policy: ScrollBar.AsNeeded
            height: timelineViewRoot.scrollBarThickness
        }

        ScrollBar.vertical: ScrollBar {
            id: verticalScrollBar

            policy: ScrollBar.AsNeeded
            width: timelineViewRoot.scrollBarThickness
        }
    }

    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        onWheel: (event) => {
            var percent = timelineViewRoot.scaleToZoomPercent(timelineViewRoot.scale);
            percent = timelineViewRoot.clamp(percent + (event.angleDelta.y > 0 ? 10 : -10), 10, 400);
            TimelineBridge.setZoom(timelineViewRoot.zoomPercentToScale(percent));
        }
    }

    WheelHandler {
        acceptedModifiers: Qt.NoModifier
        onWheel: (event) => {
            timelineViewRoot.autoScrollSuspended = true;
            var maxY = Math.max(0, timelineFlickable.contentHeight - timelineFlickable.height);
            timelineFlickable.contentY = timelineViewRoot.clamp(
                timelineFlickable.contentY - event.angleDelta.y, 0, maxY);
        }
    }
}
