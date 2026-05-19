import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: rulerRoot

    // 外部から注入される依存プロパティ
    property var targetFlickable: null
    property int rulerHeight: 32
    property int timeWidth: 60
    property double fps: 60
    property alias canvas: rulerCanvas

    // ユーティリティ
    function pxToFrame(px, contentX) {
        var scale = Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1;
        var x = px + contentX;
        return Math.max(0, Math.round(x / scale));
    }

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v));
    }

    function zoomAt(wheel, zoomFactor) {
        if (!Workspace.currentTimeline || !targetFlickable)
            return ;

        var oldScale = Workspace.currentTimeline.timelineScale;
        var newScale = clamp(oldScale * zoomFactor, 0.1, 20);
        if (Math.abs(newScale - oldScale) < 1e-06)
            return ;

        var mouseX = wheel.x !== undefined ? wheel.x : (wheel.position ? wheel.position.x : 0);
        var anchorFrame = (targetFlickable.contentX + mouseX - timeWidth) / oldScale; // timeWidth補正
        Workspace.currentTimeline.timelineScale = newScale;
        // ズーム後の位置補正
        var newContentX = anchorFrame * newScale - mouseX + timeWidth;
        var maxX = Math.max(0, targetFlickable.contentWidth - targetFlickable.width);
        targetFlickable.contentX = clamp(newContentX, 0, maxX);
    }

    Layout.fillWidth: true
    Layout.preferredHeight: rulerHeight
    color: palette.window
    z: 10

    // Canvas再描画トリガー
    Connections {
        function onContentXChanged() {
            rulerCanvas.requestPaint();
        }

        target: targetFlickable
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // 左上の空白（レイヤーヘッダーの上）
        Rectangle {
            Layout.preferredWidth: timeWidth
            Layout.fillHeight: true
            color: palette.base
            border.color: palette.mid
            border.width: 1
            z: 100

            Text {
                anchors.centerIn: parent
                text: Workspace.currentTimeline && Workspace.currentTimeline.transport ? Workspace.currentTimeline.transport.currentFrame + "f" : "0f"
                font.pixelSize: 11
                font.bold: true
                color: palette.highlight
            }

        }

        // 定規本体
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

                    var viewWidth = width;
                    var viewOffsetX = offsetX;
                    // 簡易的なグリッド描画（詳細は元のロジックを維持）
                    var frameInterval = 60;
                    // 仮
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
                    ctx.strokeStyle = palette.text;
                    ctx.fillStyle = palette.text;
                    ctx.lineWidth = 1;
                    ctx.font = "10px sans-serif";
                    for (var f = alignedStart; f <= endFrame; f += frameInterval) {
                        var pixelX = f * scale - viewOffsetX;
                        var isSecond = (f % fpsInt === 0);
                        // 大目盛
                        ctx.beginPath();
                        ctx.moveTo(pixelX, 15);
                        ctx.lineTo(pixelX, height);
                        ctx.stroke();
                        if (isSecond) {
                            // 時:分:秒 形式の時間表示
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
                            ctx.fillStyle = palette.text;
                            ctx.fillText(timeLabel, pixelX + 3, 12);
                            // フレーム番号を小さく表示
                            ctx.font = "8px sans-serif";
                            ctx.fillStyle = palette.mid;
                            ctx.fillText(f + "f", pixelX + 3, 24);
                            ctx.font = "10px sans-serif";
                        } else {
                            // フレーム数テキスト（秒単位以外）
                            ctx.fillStyle = palette.text;
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
            // マウス操作（スクラブ & ズーム）

            // マウス操作（スクラブ & ズーム）
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
