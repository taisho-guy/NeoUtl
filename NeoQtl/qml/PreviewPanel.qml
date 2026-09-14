import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import NeoQtl 1.0

Rectangle {
    id: root
    color: "#0f121b"

    property int currentFrame: 0
    property int totalFrames: 1800
    property int fps: 60
    property bool isPlaying: false
    property real seekPosition: totalFrames > 0 ? (currentFrame / totalFrames) : 0.0

    signal playToggled()
    signal frameSeekRequested(int frame)
    signal stepFrameRequested(int delta)

    function formatTimecode(frame, fps) {
        if (fps <= 0) fps = 60;
        var totalSecs = Math.floor(frame / fps);
        var f = frame % fps;
        var s = totalSecs % 60;
        var m = Math.floor(totalSecs / 60) % 60;
        var h = Math.floor(totalSecs / 3600);
        function pad(n) { return (n < 10 ? "0" : "") + n; }
        return pad(h) + ":" + pad(m) + ":" + pad(s) + ":" + pad(f);
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

                Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            Rectangle {
                anchors.centerIn: parent
                width: Math.min(parent.width - 24, (parent.height - 24) * 16 / 9)
                height: width * 9 / 16
                color: "black"
                border.color: "#2a3147"
                border.width: 1

                WgpuRhiItem {
                    id: gpuItem
                    anchors.fill: parent
                    Timer {
                        interval: 16
                        running: true
                        repeat: true
                        onTriggered: gpuItem.update()
                    }
                }

                                RowLayout {
                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.margins: 12
                    spacing: 8
                    Rectangle {
                        color: "#cc0d111a"
                        radius: 4
                        implicitWidth: resLabel.implicitWidth + 12
                        implicitHeight: resLabel.implicitHeight + 6
                        Label {
                            id: resLabel
                            anchors.centerIn: parent
                            text: "1920×1080 @ " + root.fps + "fps"
                            color: "#8ab4f8"
                            font.pixelSize: 11
                            font.bold: true
                        }
                    }
                }
            }
        }

                Rectangle {
            Layout.fillWidth: true
            height: 56
            color: "#151924"
            border.color: "#222738"
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                spacing: 12

                                Button {
                    text: "⏮"
                    implicitWidth: 34; implicitHeight: 34
                    onClicked: root.frameSeekRequested(0)
                    ToolTip.visible: hovered; ToolTip.text: qsTr("先頭へ")
                }
                Button {
                    text: "◀"
                    implicitWidth: 34; implicitHeight: 34
                    onClicked: root.stepFrameRequested(-1)
                    ToolTip.visible: hovered; ToolTip.text: qsTr("前のフレーム (-1)")
                }
                Button {
                    text: root.isPlaying ? "⏸" : "▶"
                    highlighted: true
                    implicitWidth: 40; implicitHeight: 34
                    onClicked: root.playToggled()
                    ToolTip.visible: hovered; ToolTip.text: root.isPlaying ? qsTr("一時停止") : qsTr("再生")
                }
                Button {
                    text: "▶"
                    implicitWidth: 34; implicitHeight: 34
                    onClicked: root.stepFrameRequested(1)
                    ToolTip.visible: hovered; ToolTip.text: qsTr("次のフレーム (+1)")
                }
                Button {
                    text: "⏭"
                    implicitWidth: 34; implicitHeight: 34
                    onClicked: root.frameSeekRequested(root.totalFrames)
                    ToolTip.visible: hovered; ToolTip.text: qsTr("末尾へ")
                }

                                Rectangle {
                    color: "#0f121b"
                    border.color: "#272d40"
                    radius: 4
                    implicitWidth: 120
                    implicitHeight: 32
                    Label {
                        anchors.centerIn: parent
                        text: root.formatTimecode(root.currentFrame, root.fps)
                        font.family: "Monospace"
                        font.pixelSize: 13
                        font.bold: true
                        color: "#5eead4"
                    }
                }

                                Slider {
                    id: seekSlider
                    Layout.fillWidth: true
                    from: 0
                    to: root.totalFrames
                    value: root.currentFrame
                    stepSize: 1
                    onMoved: root.frameSeekRequested(Math.round(value))
                }

                                Label {
                    text: root.currentFrame + " / " + root.totalFrames + " F"
                    font.family: "Monospace"
                    font.pixelSize: 12
                    color: "#94a3b8"
                }
            }
        }
    }
}
