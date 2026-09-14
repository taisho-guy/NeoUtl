import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Rectangle {
    id: root
    color: "#131620"

    property int currentFrame: 0
    property int totalFrames: 1800
    property int fps: 60
    property real pxPerFrame: 2.0
    property int selectedTrack: 0
    property int selectedClipId: -1

    signal frameSeekRequested(int frame)
    signal clipSelected(int clipId, string name, string kind, int startFrame, int lengthFrame)

    property var clips: [
        { id: 1, track: 0, start: 0, length: 300, name: "背景動画.mp4", kind: "video", color: "#3b82f6" },
        { id: 2, track: 1, start: 60, length: 240, name: "タイトルテキスト", kind: "text", color: "#10b981" },
        { id: 3, track: 2, start: 120, length: 180, name: "BGM.wav", kind: "audio", color: "#f59e0b" },
        { id: 4, track: 1, start: 360, length: 300, name: "図形 (円)", kind: "shape", color: "#ec4899" },
        { id: 5, track: 0, start: 300, length: 450, name: "シーン2.mp4", kind: "video", color: "#3b82f6" }
    ]

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

                Rectangle {
            Layout.fillWidth: true
            height: 38
            color: "#1a1e2d"
            border.color: "#272d42"
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                spacing: 12

                Label {
                    text: qsTr("タイムライン")
                    font.bold: true
                    color: "#cbd5e1"
                }

                Button {
                    text: qsTr("+ レイヤー追加")
                    implicitHeight: 26
                }

                Button {
                    text: qsTr("クリップ分割")
                    implicitHeight: 26
                }

                Button {
                    text: qsTr("削除")
                    implicitHeight: 26
                }

                Item { Layout.fillWidth: true }

                Label { text: qsTr("ズーム:"); color: "#94a3b8"; font.pixelSize: 11 }
                Slider {
                    from: 0.5
                    to: 10.0
                    value: root.pxPerFrame
                    implicitWidth: 120
                    onMoved: root.pxPerFrame = value
                }
            }
        }

                RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

                        Column {
                Layout.preferredWidth: 100
                Layout.fillHeight: true

                                Rectangle {
                    width: parent.width
                    height: 28
                    color: "#161926"
                    border.color: "#272d42"
                    Label {
                        anchors.centerIn: parent
                        text: qsTr("トラック")
                        color: "#64748b"
                        font.pixelSize: 11
                    }
                }

                Repeater {
                    model: 5
                    Rectangle {
                        width: 100
                        height: 48
                        color: index % 2 === 0 ? "#1c2030" : "#171a27"
                        border.color: "#272d42"
                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            Label {
                                text: "Layer " + (index + 1)
                                color: "#cbd5e1"
                                font.pixelSize: 12
                            }
                        }
                    }
                }
            }

                        Flickable {
                id: timelineFlickable
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: Math.max(width, root.totalFrames * root.pxPerFrame + 200)
                contentHeight: 28 + 5 * 48
                clip: true

                                Rectangle {
                    id: ruler
                    width: timelineFlickable.contentWidth
                    height: 28
                    color: "#191d2c"
                    border.color: "#272d42"

                    Canvas {
                        anchors.fill: parent
                        onPaint: {
                            var ctx = getContext("2d");
                            ctx.clearRect(0, 0, width, height);
                            ctx.strokeStyle = "#475569";
                            ctx.fillStyle = "#94a3b8";
                            ctx.font = "10px monospace";
                            var step = root.fps;                             var stepPx = step * root.pxPerFrame;
                            if (stepPx < 40) step *= 5;

                            for (var f = 0; f < root.totalFrames; f += step) {
                                var x = f * root.pxPerFrame;
                                ctx.beginPath();
                                ctx.moveTo(x, height);
                                ctx.lineTo(x, height - 12);
                                ctx.stroke();
                                ctx.fillText(f + "F", x + 3, 14);
                            }
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            var frame = Math.round(mouse.x / root.pxPerFrame);
                            root.frameSeekRequested(Math.max(0, Math.min(root.totalFrames, frame)));
                        }
                    }
                }

                                Item {
                    y: 28
                    width: timelineFlickable.contentWidth
                    height: 5 * 48

                    Repeater {
                        model: 5
                        Rectangle {
                            y: index * 48
                            width: timelineFlickable.contentWidth
                            height: 48
                            color: index % 2 === 0 ? "#181c2b" : "#141724"
                            border.color: "#1e2336"
                        }
                    }

                                        Repeater {
                        model: root.clips
                        Rectangle {
                            id: clipRect
                            x: modelData.start * root.pxPerFrame
                            y: modelData.track * 48 + 4
                            width: Math.max(16, modelData.length * root.pxPerFrame)
                            height: 40
                            radius: 4
                            color: modelData.color
                            border.color: root.selectedClipId === modelData.id ? "#ffffff" : Qt.darker(modelData.color, 1.3)
                            border.width: root.selectedClipId === modelData.id ? 2 : 1
                            opacity: 0.9

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 8
                                anchors.rightMargin: 8
                                Label {
                                    text: modelData.name
                                    color: "white"
                                    font.bold: true
                                    font.pixelSize: 11
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                onClicked: {
                                    root.selectedClipId = modelData.id;
                                    root.clipSelected(modelData.id, modelData.name, modelData.kind, modelData.start, modelData.length);
                                }
                            }
                        }
                    }

                                        Rectangle {
                        id: playhead
                        x: root.currentFrame * root.pxPerFrame - 1
                        y: -28
                        width: 2
                        height: 5 * 48 + 28
                        color: "#ef4444"
                        z: 10

                        Rectangle {
                            anchors.top: parent.top
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: 10
                            height: 10
                            color: "#ef4444"
                            rotation: 45
                        }
                    }
                }
            }
        }
    }
}
