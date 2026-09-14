import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "./Components"

Window {
    id: timelineWindow
    width: 720
    height: 540
    minimumWidth: 480
    minimumHeight: 320
    title: "NeoUtl - 拡張編集"
    color: Theme.bg

    property int currentFrame: 0
    property int totalFrames: 300
    property real zoomScale: 1.0
    property int selectedObjectId: -1
    property int layerCount: 16

    signal seekRequested(int frame)
    signal clipSelected(int objectId)
    signal clipMoved(int objectId, int startFrame, int layer)
    signal clipSplit(int objectId, int frame)
    signal clipDeleted(int objectId)
    signal objectAddRequested(int kind, int frame, int layer)

        property var clips: [
        { id: 1, name: "Text 1", kind: 2, start: 0, end: 90, layer: 0 },
        { id: 2, name: "Shape 1", kind: 4, start: 30, end: 150, layer: 1 },
        { id: 3, name: "Audio 1", kind: 1, start: 0, end: 240, layer: 2 }
    ]

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

                Rectangle {
            Layout.fillWidth: true
            height: 28
            color: Theme.bgDark
            border.color: Theme.border
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                spacing: 4

                Rectangle {
                    width: 90
                    height: 24
                    color: Theme.card
                    border.color: Theme.border
                    radius: 2
                    Text {
                        anchors.centerIn: parent
                        text: "Scene 1"
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        font.bold: true
                    }
                }

                NeoButton {
                    text: "+"
                    outline: true
                    implicitWidth: 24
                    implicitHeight: 24
                }

                Item { Layout.fillWidth: true }
            }
        }

                Rectangle {
            id: rulerRect
            Layout.fillWidth: true
            height: 32
            color: Theme.card
            border.color: Theme.border
            border.width: 1

                        Rectangle {
                id: rulerHeaderPadding
                width: 60
                height: parent.height
                color: Theme.card
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: "Layer"
                    color: Theme.textMuted
                    font.family: Theme.fontFamily
                    font.pixelSize: 11
                }
            }

                        Item {
                anchors.left: rulerHeaderPadding.right
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                clip: true

                Canvas {
                    id: rulerCanvas
                    anchors.fill: parent
                    onPaint: {
                        var ctx = getContext("2d");
                        ctx.clearRect(0, 0, width, height);
                        ctx.strokeStyle = Theme.border;
                        ctx.fillStyle = Theme.textMuted;
                        ctx.font = "10px " + Theme.monoFont;

                        var step = Math.max(10, Math.round(50 / timelineWindow.zoomScale));
                        for (var f = 0; f <= timelineWindow.totalFrames; f += 5) {
                            var x = (f * timelineWindow.zoomScale) - timelineFlick.contentX;
                            if (x < 0 || x > width) continue;

                            var isMajor = (f % 30 === 0);
                            ctx.beginPath();
                            ctx.moveTo(x, height);
                            ctx.lineTo(x, height - (isMajor ? 14 : 6));
                            ctx.stroke();

                            if (isMajor) {
                                ctx.fillText(f.toString(), x + 3, 16);
                            }
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: mouse => {
                        var f = Math.round((mouse.x + timelineFlick.contentX) / timelineWindow.zoomScale);
                        timelineWindow.currentFrame = Math.max(0, Math.min(timelineWindow.totalFrames, f));
                        timelineWindow.seekRequested(timelineWindow.currentFrame);
                    }
                    onPositionChanged: mouse => {
                        if (pressed) {
                            var f = Math.round((mouse.x + timelineFlick.contentX) / timelineWindow.zoomScale);
                            timelineWindow.currentFrame = Math.max(0, Math.min(timelineWindow.totalFrames, f));
                            timelineWindow.seekRequested(timelineWindow.currentFrame);
                        }
                    }
                }
            }
        }

                Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

                        Rectangle {
                id: layerHeaderArea
                width: 60
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                color: Theme.card
                border.color: Theme.border
                clip: true

                Column {
                    y: -timelineFlick.contentY
                    spacing: 0
                    Repeater {
                        model: timelineWindow.layerCount
                        delegate: Rectangle {
                            width: 60
                            height: 30
                            color: (index % 2 === 0) ? Theme.card : Theme.bgDark
                            border.color: Theme.borderMuted
                            border.width: 1

                            Text {
                                anchors.centerIn: parent
                                text: "L" + (index + 1)
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 11
                            }
                        }
                    }
                }
            }

                        Flickable {
                id: timelineFlick
                anchors.left: layerHeaderArea.right
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                contentWidth: Math.max(width, (timelineWindow.totalFrames + 50) * timelineWindow.zoomScale)
                contentHeight: timelineWindow.layerCount * 30
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                onContentXChanged: rulerCanvas.requestPaint()

                                Canvas {
                    id: gridCanvas
                    anchors.fill: parent
                    onPaint: {
                        var ctx = getContext("2d");
                        ctx.clearRect(0, 0, width, height);

                                                ctx.strokeStyle = Theme.borderMuted;
                        for (var l = 0; l <= timelineWindow.layerCount; l++) {
                            ctx.beginPath();
                            ctx.moveTo(0, l * 30);
                            ctx.lineTo(width, l * 30);
                            ctx.stroke();
                        }

                                                for (var f = 0; f <= timelineWindow.totalFrames; f += 30) {
                            var x = f * timelineWindow.zoomScale;
                            ctx.beginPath();
                            ctx.moveTo(x, 0);
                            ctx.lineTo(x, height);
                            ctx.stroke();
                        }
                    }
                }

                                Repeater {
                    model: timelineWindow.clips
                    delegate: Rectangle {
                        id: clipRect
                        x: modelData.start * timelineWindow.zoomScale
                        y: modelData.layer * 30 + 3
                        width: Math.max(12, (modelData.end - modelData.start) * timelineWindow.zoomScale)
                        height: 24
                        radius: 3
                        color: Theme.clipColor(modelData.kind)
                        border.color: (timelineWindow.selectedObjectId === modelData.id) ? "#ffffff" : Theme.border
                        border.width: (timelineWindow.selectedObjectId === modelData.id) ? 2 : 1

                        Text {
                            anchors.fill: parent
                            anchors.leftMargin: 6
                            anchors.rightMargin: 6
                            text: modelData.name
                            color: "#ffffff"
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                            font.bold: true
                            verticalAlignment: Text.AlignVCenter
                            elide: Text.ElideRight
                        }

                        MouseArea {
                            id: clipMouse
                            anchors.fill: parent
                            drag.target: clipRect
                            drag.axis: Drag.XAndYAxis
                            onClicked: {
                                timelineWindow.selectedObjectId = modelData.id;
                                timelineWindow.clipSelected(modelData.id);
                            }
                            onReleased: {
                                var newStart = Math.max(0, Math.round(clipRect.x / timelineWindow.zoomScale));
                                var newLayer = Math.max(0, Math.min(timelineWindow.layerCount - 1, Math.round((clipRect.y - 3) / 30)));
                                clipRect.x = newStart * timelineWindow.zoomScale;
                                clipRect.y = newLayer * 30 + 3;
                                timelineWindow.clipMoved(modelData.id, newStart, newLayer);
                            }
                        }
                    }
                }

                                Rectangle {
                    x: timelineWindow.currentFrame * timelineWindow.zoomScale
                    y: 0
                    width: 2
                    height: timelineFlick.contentHeight
                    color: Theme.playhead
                    z: 10
                }
            }
        }
    }
}
