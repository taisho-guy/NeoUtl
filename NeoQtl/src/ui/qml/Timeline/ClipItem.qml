import QtQuick
import ".."

Item {
    id: clipDelegate

    property int layerHeight: 30
    property int layerCount: 128
    property int clipResizeHandleWidth: 6
    property Item flickableContentItem: null
    property var snapFrameFunc: function(f, ignoreSnap) {
        return Math.max(0, Math.round(f));
    }
    property int resizeDraftStart: -1
    property int resizeDraftDuration: -1
    property real scale: 1
    property var timelineViewRoot: null

    readonly property int clipId: modelData.id
    readonly
    property bool isSelected: modelData.selected
    readonly property bool isLocked: modelData.locked
    readonly
    property int baseStartFrame: modelData.startFrame
    readonly property int baseDuration: modelData.durationFrames
    readonly
    property int baseLayer: modelData.layer

    property int dragDeltaStart: (isSelected && timelineViewRoot && timelineViewRoot.isDraggingMulti)
        ? timelineViewRoot.activeDragDeltaFrame : 0
    property int dragDeltaLayer: (isSelected && timelineViewRoot && timelineViewRoot.isDraggingMulti)
        ? timelineViewRoot.activeDragDeltaLayer : 0

    readonly property int effectiveStart: resizeDraftStart >= 0
        ? resizeDraftStart : Math.max(0, baseStartFrame + dragDeltaStart)
    readonly
    property int effectiveDuration: resizeDraftDuration >= 0
        ? resizeDraftDuration : baseDuration
    readonly property int effectiveLayer: Math.max(0,
        Math.min(layerCount - 1, baseLayer + dragDeltaLayer))

    signal clipMoved(int clipId, int deltaLayer, int deltaStartFrame, int duration)
    signal clipResized(int clipId, int deltaStartFrame, int deltaDuration, int unused)
    signal clipDoubleClicked(int clipId)
    signal clipContextRequested(int clipId, int frame, int layer)

    x: effectiveStart * scale
    y: effectiveLayer * layerHeight
    width: Math.max(4, effectiveDuration * scale)
    height: layerHeight

    Rectangle {
        id: clipBody

        anchors.fill: parent
        anchors.topMargin: 3
        anchors.bottomMargin: 3
        radius: 3
        border.color: clipDelegate.isSelected ? "#ffffff" : Theme.border
        border.width: clipDelegate.isSelected ? 2 : 1
        opacity: clipDelegate.isLocked ? 0.55 : 1

        gradient: Gradient {
            orientation: Gradient.Horizontal

            GradientStop {
                position: 0
                color: Qt.darker(Theme.clipColor(modelData.colorIndex), 1.6)
            }

            GradientStop {
                position: 1
                color: modelData.kindKnown
                    ? Theme.clipColor(modelData.colorIndex) : "#5a5a5a"
            }
        }

        Text {
            id: clipLabel
    property real stickyX: Math.max(0,
                (clipDelegate.timelineViewRoot ? clipDelegate.timelineViewRoot.contentX : 0)
                - clipDelegate.x)

            x: Math.min(stickyX + 6, clipBody.width - 4)
            anchors.verticalCenter: parent.verticalCenter
            width: clipBody.width - x - 4
            text: modelData.label
            color: "#ffffff"
            font.family: Theme.fontFamily
            font.pixelSize: 11
            font.bold: true
            elide: Text.ElideRight
            visible: clipBody.width > 20
        }

        Repeater {
            model: modelData.keyframes || []

            delegate: Rectangle {
                width: 8
                height: 8
                radius: 1
                color: Theme.playhead
                border.color: Qt.darker(Theme.playhead, 1.5)
                border.width: 1
                rotation: 45
                x: (modelData - clipDelegate.effectiveStart)
                    * clipBody.width / Math.max(1, clipDelegate.effectiveDuration) - 4
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        MouseArea {
            id: bodyMouse

            property point pressScenePos: Qt.point(0, 0)
    property int initialLayer: 0
            property int initialFrame: 0
    property int lastProposedFrame: -1
            property int lastProposedLayer: -1
    property bool dragActive: false
            property real dragThreshold: 3
    property point lastScenePos: Qt.point(0, 0)
            property int lastModifiers: 0

            function updateDragFromScenePos(sp, modifiers) {
                var deltaX = sp.x - pressScenePos.x;
                var deltaY = sp.y - pressScenePos.y;
                var rawFrame = initialFrame + deltaX / clipDelegate.scale;
                var proposedFrame = clipDelegate.snapFrameFunc(rawFrame,
                    (modifiers & Qt.ShiftModifier) !== 0);
                var proposedLayer = Math.max(0, Math.min(clipDelegate.layerCount - 1,
                    initialLayer + Math.round(deltaY / clipDelegate.layerHeight)));
                if (proposedFrame === lastProposedFrame && proposedLayer === lastProposedLayer)
                    return ;

                lastProposedFrame = proposedFrame;
                lastProposedLayer = proposedLayer;
                if (clipDelegate.timelineViewRoot) {
                    clipDelegate.timelineViewRoot.activeDragDeltaFrame =
                        proposedFrame - clipDelegate.baseStartFrame;
                    clipDelegate.timelineViewRoot.activeDragDeltaLayer =
                        proposedLayer - clipDelegate.baseLayer;
                }
            }

            anchors.fill: parent
            anchors.leftMargin: clipDelegate.clipResizeHandleWidth
            anchors.rightMargin: clipDelegate.clipResizeHandleWidth
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            enabled: !clipDelegate.isLocked
            cursorShape: dragActive ? Qt.ClosedHandCursor : Qt.OpenHandCursor
            preventStealing: true

            onPressed: (mouse) => {
                if (mouse.button === Qt.RightButton) {
                    clipDelegate.clipContextRequested(clipDelegate.clipId,
                        clipDelegate.baseStartFrame, clipDelegate.baseLayer);
                    return ;
                }
                if (!clipDelegate.isSelected)
                    TimelineBridge.selectClip(clipDelegate.clipId,
                        (mouse.modifiers & Qt.ControlModifier) !== 0);

                pressScenePos = mapToItem(clipDelegate.flickableContentItem, mouse.x, mouse.y);
                lastScenePos = pressScenePos;
                lastModifiers = mouse.modifiers;
                initialLayer = clipDelegate.baseLayer;
                initialFrame = clipDelegate.baseStartFrame;
                lastProposedFrame = -1;
                lastProposedLayer = -1;
                dragActive = false;
                if (clipDelegate.timelineViewRoot) {
                    clipDelegate.timelineViewRoot.beginDragAutoScroll(function() {
                        bodyMouse.updateDragFromScenePos(bodyMouse.lastScenePos,
                            bodyMouse.lastModifiers);
                    });
                }
            }

            onPositionChanged: (mouse) => {
                if (!pressed)
                    return ;

                var sp = mapToItem(clipDelegate.flickableContentItem, mouse.x, mouse.y);
                lastScenePos = sp;
                lastModifiers = mouse.modifiers;
                if (!dragActive) {
                    var moved = Math.abs(sp.x - pressScenePos.x) + Math.abs(sp.y - pressScenePos.y);
                    if (moved < dragThreshold)
                        return ;

                    dragActive = true;
                    if (clipDelegate.timelineViewRoot)
                        clipDelegate.timelineViewRoot.isDraggingMulti = true;

                }
                if (clipDelegate.timelineViewRoot) {
                    clipDelegate.timelineViewRoot.updateDragAutoScroll(
                        mapToItem(clipDelegate.timelineViewRoot, mouse.x, mouse.y));
                }
                updateDragFromScenePos(sp, mouse.modifiers);
            }

            onReleased: (mouse) => {
                if (clipDelegate.timelineViewRoot)
                    clipDelegate.timelineViewRoot.endDragAutoScroll();

                if (!dragActive) {
                    if (mouse.button === Qt.LeftButton)
                        TimelineBridge.selectClip(clipDelegate.clipId,
                            (mouse.modifiers & Qt.ControlModifier) !== 0);

                    return ;
                }
                dragActive = false;
                var dFrame = 0;
                var dLayer = 0;
                if (clipDelegate.timelineViewRoot) {
                    dFrame = clipDelegate.timelineViewRoot.activeDragDeltaFrame;
                    dLayer = clipDelegate.timelineViewRoot.activeDragDeltaLayer;
                    clipDelegate.timelineViewRoot.isDraggingMulti = false;
                    clipDelegate.timelineViewRoot.activeDragDeltaFrame = 0;
                    clipDelegate.timelineViewRoot.activeDragDeltaLayer = 0;
                }
                if (dFrame !== 0 || dLayer !== 0)
                    clipDelegate.clipMoved(clipDelegate.clipId, dLayer, dFrame,
                        clipDelegate.baseDuration);

            }

            onDoubleClicked: clipDelegate.clipDoubleClicked(clipDelegate.clipId)
        }

        Rectangle {
            id: leftHandle

            width: clipDelegate.clipResizeHandleWidth
            height: parent.height
            anchors.left: parent.left
            color: leftMouse.containsMouse ? Qt.rgba(1, 1, 1, 0.3) : "transparent"

            MouseArea {
                id: leftMouse

                property real startSceneX: 0
    property int startFrame: 0
                property int startDuration: 0
    property bool resizing: false

                anchors.fill: parent
                enabled: !clipDelegate.isLocked
                hoverEnabled: true
                cursorShape: Qt.SizeHorCursor
                preventStealing: true

                onPressed: (mouse) => {
                    startSceneX = mapToItem(clipDelegate.flickableContentItem, mouse.x, 0).x;
                    startFrame = clipDelegate.baseStartFrame;
                    startDuration = clipDelegate.baseDuration;
                    resizing = true;
                }

                onPositionChanged: (mouse) => {
                    if (!resizing)
                        return ;

                    var sx = mapToItem(clipDelegate.flickableContentItem, mouse.x, 0).x;
                    var raw = startFrame + (sx - startSceneX) / clipDelegate.scale;
                    var newStart = clipDelegate.snapFrameFunc(raw,
                        (mouse.modifiers & Qt.ShiftModifier) !== 0);
                    var newDur = startDuration + (startFrame - newStart);
                    if (newDur < 1) {
                        newDur = 1;
                        newStart = startFrame + startDuration - 1;
                    }
                    clipDelegate.resizeDraftStart = Math.max(0, newStart);
                    clipDelegate.resizeDraftDuration = newDur;
                }

                onReleased: {
                    if (!resizing)
                        return ;

                    resizing = false;
                    var dStart = clipDelegate.resizeDraftStart - startFrame;
                    var dDur = clipDelegate.resizeDraftDuration - startDuration;
                    clipDelegate.resizeDraftStart = -1;
                    clipDelegate.resizeDraftDuration = -1;
                    if (dStart !== 0 || dDur !== 0)
                        clipDelegate.clipResized(clipDelegate.clipId, dStart, dDur, 0);

                }
            }
        }

        Rectangle {
            id: rightHandle

            width: clipDelegate.clipResizeHandleWidth
            height: parent.height
            anchors.right: parent.right
            color: rightMouse.containsMouse ? Qt.rgba(1, 1, 1, 0.3) : "transparent"

            MouseArea {
                id: rightMouse

                property real startSceneX: 0
    property int startFrame: 0
                property int startDuration: 0
    property bool resizing: false

                anchors.fill: parent
                enabled: !clipDelegate.isLocked
                hoverEnabled: true
                cursorShape: Qt.SizeHorCursor
                preventStealing: true

                onPressed: (mouse) => {
                    startSceneX = mapToItem(clipDelegate.flickableContentItem, mouse.x, 0).x;
                    startFrame = clipDelegate.baseStartFrame;
                    startDuration = clipDelegate.baseDuration;
                    resizing = true;
                }

                onPositionChanged: (mouse) => {
                    if (!resizing)
                        return ;

                    var sx = mapToItem(clipDelegate.flickableContentItem, mouse.x, 0).x;
                    var rawEnd = startFrame + startDuration + (sx - startSceneX) / clipDelegate.scale;
                    var newEnd = clipDelegate.snapFrameFunc(rawEnd,
                        (mouse.modifiers & Qt.ShiftModifier) !== 0);
                    clipDelegate.resizeDraftStart = startFrame;
                    clipDelegate.resizeDraftDuration = Math.max(1, newEnd - startFrame);
                }

                onReleased: {
                    if (!resizing)
                        return ;

                    resizing = false;
                    var dDur = clipDelegate.resizeDraftDuration - startDuration;
                    clipDelegate.resizeDraftStart = -1;
                    clipDelegate.resizeDraftDuration = -1;
                    if (dDur !== 0)
                        clipDelegate.clipResized(clipDelegate.clipId, 0, dDur, 0);

                }
            }
        }
    }
}
