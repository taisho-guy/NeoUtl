import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common

Common.AviQtlWindow {
    id: root

    title: qsTr("パッケージマネージャー")
    width: 600
    height: 400
    minimumWidth: 500
    minimumHeight: 300

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        // リポジトリ管理
        GroupBox {
            title: qsTr("リポジトリ設定")
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true

                    TextField {
                        id: repoUrlField

                        Layout.fillWidth: true
                        placeholderText: "https://example.com/repo.json"
                        selectByMouse: true
                        onAccepted: addRepoBtn.clicked()
                    }

                    Button {
                        id: addRepoBtn

                        text: qsTr("追加")
                        enabled: repoUrlField.text.length > 0
                        onClicked: {
                            PackageManager.addRepository(repoUrlField.text);
                            repoUrlField.text = "";
                        }
                    }

                }

                ListView {
                    id: repoListView

                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.min(contentHeight, 80)
                    clip: true
                    model: PackageManager ? PackageManager.repositories : []

                    delegate: ItemDelegate {
                        width: repoListView.width
                        height: 32
                        padding: 0

                        contentItem: RowLayout {
                            Label {
                                text: modelData
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                                leftPadding: 8
                            }

                            Button {
                                flat: true
                                Layout.preferredWidth: 32
                                Layout.fillHeight: true
                                onClicked: PackageManager.removeRepository(modelData)

                                contentItem: Common.AviQtlIcon {
                                    iconName: "delete_bin_line"
                                    size: 14
                                    color: parent.hovered ? "red" : palette.text
                                }

                            }

                        }

                    }

                }

            }

        }

        RowLayout {
            Layout.fillWidth: true

            Button {
                text: qsTr("リポジトリを同期")
                icon.name: "refresh-line"
                enabled: PackageManager && !PackageManager.isBusy
                onClicked: PackageManager.refreshRepositories()
            }

            Item {
                Layout.fillWidth: true
            }

            TextField {
                placeholderText: qsTr("検索...")
                Layout.preferredWidth: 200
            }

        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: palette.base
            border.color: palette.mid
            border.width: 1

            Label {
                anchors.centerIn: parent
                text: PackageManager && PackageManager.isBusy ? PackageManager.statusText : qsTr("（後で実装: パッケージリスト）")
                color: palette.text
            }

        }

        ProgressBar {
            Layout.fillWidth: true
            visible: PackageManager && PackageManager.isBusy
            value: PackageManager ? PackageManager.progress : 0
        }

    }

}
