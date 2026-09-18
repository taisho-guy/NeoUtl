import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "./Components"

Window {
    id: previewWindow
    width: 720
    height: 540
    minimumWidth: 400
    minimumHeight: 300
    title: "NeoUtl"
    color: Theme.bg

    property int currentFrame: 0
    property int totalFrames: 300
    property bool isPlaying: false
    property int playbackSpeed: 100

    signal seekRequested(int frame)
    signal togglePlayRequested()
    signal stepPrevRequested()
    signal stepNextRequested()
    signal speedChanged(int speed)

    signal openExportRequested()
    signal openSystemSettingsRequested()
    signal openProjectSettingsRequested()
    signal openSceneSettingsRequested()
    signal openKeybindingsRequested()
    signal openTimelineRequested()
    signal openPropertiesRequested()

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

                MenuBar {
            id: menuBar
            Layout.fillWidth: true

            background: Rectangle {
                color: Theme.card
                border.color: Theme.border
                border.width: 1
            }

            Menu {
                title: "ファイル"
                Action { text: "新規プロジェクト" }
                Action { text: "プロジェクトを開く…" }
                Action { text: "上書き保存" }
                Action { text: "名前を付けて保存…" }
                MenuSeparator {}
                Action {
                    text: "メディアの書き出し…"
                    onTriggered: previewWindow.openExportRequested()
                }
                MenuSeparator {}
                Action {
                    text: "終了"
                    onTriggered: Qt.quit()
                }
            }

            Menu {
                title: "編集"
                Action { text: "元に戻す" }
                Action { text: "やり直し" }
                MenuSeparator {}
                Action {
                    text: "システム設定…"
                    onTriggered: previewWindow.openSystemSettingsRequested()
                }
                Action {
                    text: "プロジェクト設定…"
                    onTriggered: previewWindow.openProjectSettingsRequested()
                }
                Action {
                    text: "シーン設定…"
                    onTriggered: previewWindow.openSceneSettingsRequested()
                }
                Action {
                    text: "ショートカット設定…"
                    onTriggered: previewWindow.openKeybindingsRequested()
                }
            }

            Menu {
                title: "表示"
                Action {
                    text: "拡張編集"
                    onTriggered: previewWindow.openTimelineRequested()
                }
                Action {
                    text: "プロパティ"
                    onTriggered: previewWindow.openPropertiesRequested()
                }
            }
        }

                Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.bgDark
            clip: true

            Rectangle {
                id: previewSurfaceArea
                objectName: "previewSurfaceArea"
                anchors.fill: parent
                color: "#000000"
            }

                        Rectangle {
                anchors.fill: parent
                color: "transparent"
                border.color: Qt.rgba(255, 255, 255, 0.05)
                border.width: 1
            }
        }

                Rectangle {
            Layout.fillWidth: true
            height: 38
            color: Theme.card
            border.color: Theme.border
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                spacing: 12

                                Text {
                    id: frameCounter
                    text: {
                        var digits = Math.max(1, previewWindow.totalFrames).toString().length;
                        var pad = function(n, len) {
                            var s = n.toString();
                            while (s.length < len) s = "0" + s;
                            return s;
                        };
                        return pad(previewWindow.currentFrame, digits) + " / " + previewWindow.totalFrames;
                    }
                    font.family: Theme.monoFont
                    font.pixelSize: 13
                    color: Theme.text
                }

                Item { Layout.fillWidth: true }

                                NeoButton {
                    outline: true
                    text: "⏮"
                    implicitWidth: 32
                    implicitHeight: 28
                    onClicked: {
                        previewWindow.stepPrevRequested();
                        if (previewWindow.currentFrame > 0) previewWindow.currentFrame--;
                    }
                }

                NeoButton {
                    text: previewWindow.isPlaying ? "⏸" : "▶"
                    implicitWidth: 36
                    implicitHeight: 28
                    onClicked: {
                        previewWindow.isPlaying = !previewWindow.isPlaying;
                        previewWindow.togglePlayRequested();
                    }
                }

                NeoButton {
                    outline: true
                    text: "⏭"
                    implicitWidth: 32
                    implicitHeight: 28
                    onClicked: {
                        previewWindow.stepNextRequested();
                        if (previewWindow.currentFrame < previewWindow.totalFrames) previewWindow.currentFrame++;
                    }
                }

                Item { Layout.fillWidth: true }

                                NeoNumberInput {
                    label: "速度"
                    value: previewWindow.playbackSpeed
                    from: 10
                    to: 800
                    stepSize: 10
                    suffix: "%"
                    onValueChanged: previewWindow.speedChanged(value)
                }
            }
        }
    }
}
