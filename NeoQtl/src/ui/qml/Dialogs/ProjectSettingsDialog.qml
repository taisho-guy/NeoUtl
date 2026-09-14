import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog
    width: 520
    height: 360
    title: "プロジェクト設定"
    color: Theme.bg
    modality: Qt.WindowModal

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ColumnLayout {
                width: parent.width
                spacing: 16
                anchors.margins: 16

                NeoCard {
                    Layout.fillWidth: true
                    heading: "基本設定"
                    RowLayout {
                        width: parent.width
                        Text { text: "プロジェクト名:"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 100 }
                        NeoTextInput { id: nameInput; text: "Project"; Layout.fillWidth: true }
                    }
                }

                NeoCard {
                    Layout.fillWidth: true
                    heading: "映像フォーマット"
                    RowLayout {
                        spacing: 12
                        NeoNumberInput { label: "FPS:"; value: 30; from: 1; to: 240; suffix: "fps" }
                        NeoNumberInput { label: "幅:"; value: 1920; from: 16; to: 7680; suffix: "px" }
                        NeoNumberInput { label: "高さ:"; value: 1080; from: 16; to: 7680; suffix: "px" }
                    }
                }

                NeoCard {
                    Layout.fillWidth: true
                    heading: "音声フォーマット"
                    RowLayout {
                        spacing: 12
                        NeoNumberInput { label: "チャンネル:"; value: 2; from: 1; to: 8; suffix: "ch" }
                        NeoNumberInput { label: "レート:"; value: 48000; from: 8000; to: 192000; suffix: "Hz" }
                    }
                }
            }
        }

                Rectangle {
            Layout.fillWidth: true
            height: 48
            color: Theme.card
            border.color: Theme.border

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                spacing: 8
                Item { Layout.fillWidth: true }
                NeoButton { outline: true; text: "キャンセル"; onClicked: dialog.close() }
                NeoButton { text: "OK"; onClicked: dialog.close() }
            }
        }
    }
}
