import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog
    width: 720
    height: 540
    title: "NeoUtl - システム設定"
    color: Theme.bg
    modality: Qt.WindowModal

    property var categories: ["一般", "外観", "パフォーマンス", "デコード", "タイムライン", "音声プラグイン", "アップデート"]
    property int selectedCategory: 0

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

                        Rectangle {
                Layout.preferredWidth: 160
                Layout.fillHeight: true
                color: Theme.card
                border.color: Theme.border

                ListView {
                    anchors.fill: parent
                    anchors.margins: 4
                    spacing: 2
                    model: dialog.categories

                    delegate: Rectangle {
                        width: parent.width
                        height: 32
                        radius: Theme.radiusSm
                        color: (dialog.selectedCategory === index) ? Theme.accent :
                               mouse.containsMouse ? Theme.cardHover : "transparent"

                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData
                            color: (dialog.selectedCategory === index) ? "#ffffff" : Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                        }

                        MouseArea {
                            id: mouse
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: dialog.selectedCategory = index
                        }
                    }
                }
            }

                        ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: 16
                    anchors.margins: 16

                    Text {
                        text: dialog.categories[dialog.selectedCategory]
                        color: Theme.accent
                        font.family: Theme.fontFamily
                        font.pixelSize: 16
                        font.bold: true
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        height: 1
                        color: Theme.border
                    }

                    RowLayout {
                        spacing: 12
                        CheckBox { id: autosaveCheck; checked: true }
                        Text { text: "自動保存を有効化"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13 }
                    }

                    NeoNumberInput {
                        label: "自動保存間隔（秒）"
                        value: 60
                        from: 10
                        to: 3600
                        suffix: "秒"
                    }

                    NeoNumberInput {
                        label: "UIスケール（%）"
                        value: 100
                        from: 50
                        to: 200
                        suffix: "%"
                    }

                    NeoNumberInput {
                        label: "デコード作業スレッド数"
                        value: 8
                        from: 1
                        to: 64
                    }

                    Item { Layout.fillHeight: true }
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

                NeoButton {
                    outline: true
                    text: "キャンセル"
                    onClicked: dialog.close()
                }

                NeoButton {
                    text: "OK"
                    onClicked: dialog.close()
                }
            }
        }
    }
}
