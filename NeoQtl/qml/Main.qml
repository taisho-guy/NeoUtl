import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import NeoQtl 1.0

ApplicationWindow {
    id: root
    visible: true
    width: 1100; height: 700
    minimumWidth: 720; minimumHeight: 480
    title: "NeoQtl — Qt / QML wgpu prototype"
    color: "#10131c"
    property real seekPosition: 0.35
    property bool playing: true

    Timer { interval: 16; running: root.playing; repeat: true; onTriggered: root.seekPosition = (root.seekPosition + 0.0015) % 1.0 }
    menuBar: MenuBar {
        Menu { title: qsTr("ファイル"); MenuItem { text: qsTr("プロトタイプ") } }
        Menu { title: qsTr("表示"); MenuItem { text: root.playing ? qsTr("一時停止") : qsTr("再生"); onTriggered: root.playing = !root.playing } }
    }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true
            WgpuRhiItem {
                id: gpuItem
                anchors.fill: parent
                Timer { interval: 16; running: true; repeat: true; onTriggered: gpuItem.update() }
            }
            Rectangle { id: gradientRect; anchors.fill: parent; visible: false; property real t: root.seekPosition * 20.0; gradient: Gradient {
                GradientStop { position: 0.0; color: Qt.hsla((gradientRect.t * 0.055) % 1.0, 0.85, 0.55, 1) }
                GradientStop { position: 0.5; color: Qt.hsla((gradientRect.t * 0.055 + 0.33) % 1.0, 0.85, 0.55, 1) }
                GradientStop { position: 1.0; color: Qt.hsla((gradientRect.t * 0.055 + 0.66) % 1.0, 0.85, 0.55, 1) }
            } }
            Column {
                anchors.centerIn: parent
                spacing: 8
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("動的な虹色グラデーション")
                    color: "white"
                    font.pixelSize: 26
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "Qt / QML + Rust wgpu item"
                    color: "#e0e5f5"
                }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            height: 64
            color: "#171b28"
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 20
                anchors.rightMargin: 20
                Button {
                    text: root.playing ? "Ⅱ" : "▶"
                    onClicked: root.playing = !root.playing
                }
                Slider {
                    Layout.fillWidth: true
                    value: root.seekPosition
                    onMoved: root.seekPosition = value
                }
                Label {
                    text: Math.round(root.seekPosition * 100) + "%"
                    color: "white"
                }
            }
        }
    }
}
