import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog

    function open() { visible = true }
    function close() { visible = false }
    width: 720
    height: 540
    title: "ショートカット設定"
    color: Theme.bg
    modality: Qt.WindowModal

    property var shortcutsList: [
        { label: "新規プロジェクト", scope: "全体", key: "Ctrl+N" },
        { label: "プロジェクトを開く", scope: "全体", key: "Ctrl+O" },
        { label: "上書き保存", scope: "全体", key: "Ctrl+S" },
        { label: "元に戻す", scope: "全体", key: "Ctrl+Z" },
        { label: "やり直し", scope: "全体", key: "Ctrl+Y" },
        { label: "再生/一時停止", scope: "全体", key: "Space" },
        { label: "分割", scope: "タイムライン", key: "S" },
        { label: "削除", scope: "タイムライン", key: "Delete" }
    ]

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Text {
            text: "ショートカット一覧"
            color: Theme.accent
            font.family: Theme.fontFamily
            font.pixelSize: 16
            font.bold: true
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ListView {
                width: parent.width
                spacing: 4
                model: dialog.shortcutsList

                delegate: Rectangle {
                    width: parent.width
                    height: 36
                    radius: Theme.radiusSm
                    color: Theme.card
                    border.color: Theme.borderMuted

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12

                        Text {
                            text: modelData.label
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: 13
                            Layout.preferredWidth: 200
                        }

                        Text {
                            text: modelData.scope
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: 12
                            Layout.preferredWidth: 120
                        }

                        Item { Layout.fillWidth: true }

                        Rectangle {
                            width: keyText.implicitWidth + 16
                            height: 24
                            radius: 3
                            color: Theme.bgDark
                            border.color: Theme.border

                            Text {
                                id: keyText
                                anchors.centerIn: parent
                                text: modelData.key
                                color: Theme.accent
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                font.bold: true
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            NeoButton {
                outline: true
                text: "初期値に戻す"
            }
            Item { Layout.fillWidth: true }
            NeoButton {
                text: "閉じる"
                onClicked: dialog.close()
            }
        }
    }
}
