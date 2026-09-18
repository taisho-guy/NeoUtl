import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: rulerRoot

        property var targetFlickable: null
    property int rulerHeight: 32
    property int timeWidth: 60
    property double fps: 60
    property alias canvas: rulerCanvas
    property int timelineDuration: 0

    signal zoomRequested(int percent)

        function pxToFrame(px, contentX) {
        var scale = Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1;
        var x = px + contentX;
        return Math.max(0, Math.round(x / scale));
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

    function zoomAt(wheel, zoomFactor) {
        if (!Workspace.currentTimeline || !targetFlickable)
            return ;

        var oldScale = Workspace.currentTimeline.timelineScale;
        var minZ = SettingsManager ? SettingsManager.value("timelineZoomMin", 10) : 10;
        var maxZ = SettingsManager ? SettingsManager.value("timelineZoomMax", 400) : 400;
        var newScale = clamp(oldScale * zoomFactor, zoomPercentToScale(minZ), zoomPercentToScale(maxZ));
        if (Math.abs(newScale - oldScale) < 1e-06)
            return ;

        var mouseX = wheel.x !== undefined ? wheel.x : (wheel.position ? wheel.position.x : 0);
        var anchorFrame = (targetFlickable.contentX + mouseX - timeWidth) / oldScale;         Workspace.currentTimeline.timelineScale = newScale;
                var newContentX = anchorFrame * newScale - mouseX + timeWidth;
        var maxX = Math.max(0, targetFlickable.contentWidth - targetFlickable.width);
        targetFlickable.contentX = clamp(newContentX, 0, maxX);
    }

    Layout.fillWidth: true
    Layout.preferredHeight: rulerHeight
    color: palette.window
    z: 10

        Connections {
        function onContentXChanged() {
            rulerCanvas.requestPaint();
        }

        target: targetFlickable
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

                ComboBox {
            id: zoomCombo

            Layout.preferredWidth: timeWidth
            Layout.fillHeight: true
            z: 100
            editable: true
            flat: true
            model: [10, 25, 50, 75, 100, 150, 200, 300, 400]
            onAccepted: {
                var val = parseInt(editText);
                if (!isNaN(val))
                    rulerRoot.zoomRequested(val);

                focus = false;
            }
            onActivated: (index) => {
                rulerRoot.zoomRequested(model[index]);
            }
            Component.onCompleted: {
                if (Workspace.currentTimeline)
                    editText = Math.round(rulerRoot.scaleToZoomPercent(Workspace.currentTimeline.timelineScale)).toString();
                else
                    editText = "100";
            }

            Connections {
                function onTimelineScaleChanged() {
                    if (!zoomCombo.activeFocus)
                        zoomCombo.editText = Math.round(rulerRoot.scaleToZoomPercent(Workspace.currentTimeline.timelineScale)).toString();

                }

                target: Workspace.currentTimeline
            }

            validator: IntValidator {
                bottom: 10
                top: 400
            }

        }

                Item {
            
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            Canvas {
                id: rulerCanvas

                property double scale: Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1
    property double offsetX: targetFlickable ? targetFlickable.contentX : 0
                property int fpsInt: Math.round(rulerRoot.fps)

                anchors.fill: parent
                onScaleChanged: requestPaint()
                onOffsetXChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    if (width <= 0 || height <= 0 || scale <= 0)
                        return ;

                    var ctx = getContext("2d");
                    ctx.clearRect(-1, -1, width + 2, height + 2);
                    if (!Workspace.currentTimeline)
                        return ;

                                        var bgLuma = (0.299 * palette.window.r) + (0.587 * palette.window.g) + (0.114 * palette.window.b);
                    var drawColor = bgLuma > 0.5 ? "#000000" : "#ffffff";
                    var subColor = bgLuma > 0.5 ? "#555555" : "#aaaaaa";
                    var viewWidth = width;
                    var viewOffsetX = offsetX;
                                        var frameInterval = 60;
                                        if (scale > 5)
                        frameInterval = 10;
                    else if (scale > 1)
                        frameInterval = 30;
                    else if (scale > 0.5)
                        frameInterval = 60;
                    else
                        frameInterval = 300;
                    var startFrame = Math.floor(viewOffsetX / scale);
                    var endFrame = Math.ceil((viewOffsetX + viewWidth) / scale);
                    var alignedStart = Math.floor(startFrame / frameInterval) * frameInterval;
                    ctx.strokeStyle = drawColor;
                    ctx.fillStyle = drawColor;
                    ctx.lineWidth = 1;
                    ctx.font = "10px sans-serif";
                    for (var f = alignedStart; f <= endFrame; f += frameInterval) {
                        var pixelX = f * scale - viewOffsetX;
                        var isSecond = (f % fpsInt === 0);
                                                ctx.beginPath();
                        ctx.moveTo(pixelX, 15);
                        ctx.lineTo(pixelX, height);
                        ctx.stroke();
                        if (isSecond) {
                                                        var totalSeconds = f / fpsInt;
                            var hours = Math.floor(totalSeconds / 3600);
                            var minutes = Math.floor((totalSeconds % 3600) / 60);
                            var seconds = Math.floor(totalSeconds % 60);
                            var timeLabel;
                            if (hours > 0)
                                timeLabel = hours + ":" + ("0" + minutes).slice(-2) + ":" + ("0" + seconds).slice(-2);
                            else if (minutes > 0)
                                timeLabel = minutes + ":" + ("0" + seconds).slice(-2);
                            else
                                timeLabel = seconds + "s";
                            ctx.fillStyle = drawColor;
                            ctx.fillText(timeLabel, pixelX + 3, 12);
                                                        ctx.font = "8px sans-serif";
                            ctx.fillStyle = subColor;
                            ctx.fillText(f + "f", pixelX + 3, 24);
                            ctx.font = "10px sans-serif";
                        } else {
                                                        ctx.fillStyle = drawColor;
                            ctx.fillText(f, pixelX + 2, 12);
                        }
                    }
                }

                Connections {
                    function onCurrentTimelineChanged() {
                        rulerCanvas.requestPaint();
                    }

                    target: Workspace
                }

            }

                        Rectangle {
                id: rulerPlayhead

                x: Math.round(((Workspace.currentTimeline && Workspace.currentTimeline.transport ? Workspace.currentTimeline.transport.currentFrame : 0) * (Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1)) - (targetFlickable ? targetFlickable.contentX : 0))
                y: 0
                width: 2
                height: parent.height
                color: palette.highlight
                z: 10
            }

            Rectangle {
                id: rulerEditCursor

                visible: Workspace.currentTimeline !== null
                x: Math.round(((Workspace.currentTimeline ? Workspace.currentTimeline.cursorFrame : 0) * (Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1)) - (targetFlickable ? targetFlickable.contentX : 0))
                y: rulerRoot.height * 0.6
                width: 1
                height: rulerRoot.height * 0.4
                color: palette.highlight
                opacity: 0.7
                z: 5
            }

                        MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onPressed: (mouse) => {
                    if (mouse.button === Qt.LeftButton && targetFlickable && Workspace.currentTimeline && Workspace.currentTimeline.transport) {
                        Workspace.currentTimeline.transport.beginScrub();
                        Workspace.currentTimeline.transport.scrubTo(pxToFrame(mouse.x, targetFlickable.contentX));
                    }
                }
                onPositionChanged: (mouse) => {
                    if (pressed && (mouse.buttons & Qt.LeftButton) && targetFlickable && Workspace.currentTimeline && Workspace.currentTimeline.transport)
                        Workspace.currentTimeline.transport.scrubTo(pxToFrame(mouse.x, targetFlickable.contentX));

                }
                onReleased: (mouse) => {
                    if (mouse.button === Qt.LeftButton && Workspace.currentTimeline && Workspace.currentTimeline.transport)
                        Workspace.currentTimeline.transport.endScrub();

                }
                onWheel: (wheel) => {
                    var dy = (wheel.angleDelta.y !== 0) ? wheel.angleDelta.y : (wheel.pixelDelta.y * 10);
                    var zoomFactor = (dy > 0) ? 1.1 : 0.9;
                    zoomAt(wheel, zoomFactor);
                    wheel.accepted = true;
                }
            }

        }

    }

}
