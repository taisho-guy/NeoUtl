import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import "common" as Common

Common.AviQtlWindow {
    id: root

    property string searchQuery: ""

    title: qsTr("パッケージマネージャー")
    width: 600
    height: 400
    minimumWidth: 500
    minimumHeight: 300

    // AviQtl本体のアップデート通知ダイアログ
    MessageDialog {
        id: selfUpdateDialog

        property string newVersion: ""
        property string downloadUrl: ""

        title: qsTr("AviQtl アップデート")
        text: qsTr("新しいバージョンのAviQtl (%1) が利用可能です。\nダウンロードURL: %2\n\nアプリケーションを再起動して適用してください。").arg(newVersion).arg(downloadUrl)
        buttons: MessageDialog.Ok
    }

    // エラー通知用ダイアログ
    MessageDialog {
        id: errorDialog

        title: qsTr("パッケージマネージャーエラー")
        buttons: MessageDialog.Ok
    }

    Connections {
        function onErrorOccurred(message) {
            errorDialog.text = message;
            errorDialog.open();
        }

        function onSelfUpdateAvailable(newVersion, downloadUrl) {
            selfUpdateDialog.newVersion = newVersion;
            selfUpdateDialog.downloadUrl = downloadUrl;
            selfUpdateDialog.open();
        }

        target: PackageManager
    }

    Connections {
        function onAssetsReady(packageId, assets) {
            assetSelectionDialog.packageId = packageId;
            assetSelectionDialog.assets = assets;
            assetSelectionDialog.open();
        }

        target: PackageManager
    }

    Dialog {
        id: assetSelectionDialog

        property string packageId: ""
        property var assets: []

        title: qsTr("ダウンロードするファイルを選択")
        modal: true
        anchors.centerIn: parent
        standardButtons: Dialog.Cancel

        ListView {
            implicitWidth: 400
            implicitHeight: Math.min(300, contentHeight)
            model: assetSelectionDialog.assets
            clip: true

            delegate: ItemDelegate {
                width: parent.width
                text: modelData.name + " (" + (modelData.size / 1024 / 1024).toFixed(2) + " MB)"
                onClicked: {
                    PackageManager.installPackage(assetSelectionDialog.packageId, modelData.url);
                    assetSelectionDialog.close();
                }
            }

        }

    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

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
            // スペーサー

            Layout.fillWidth: true

            Button {
                text: qsTr("リポジトリを同期")
                icon.name: "refresh-line"
                enabled: PackageManager && !PackageManager.isBusy
                onClicked: PackageManager.refreshRepositories()
            }

            Button {
                text: qsTr("すべてアップグレード")
                icon.name: "upload-cloud-line"
                highlighted: true
                enabled: PackageManager && !PackageManager.isBusy && PackageManager.hasUpdatesAvailable
                onClicked: PackageManager.upgradeAllPackages()
            }

            Item {
                Layout.fillWidth: true
            }

            TextField {
                id: searchField

                placeholderText: qsTr("検索...")
                Layout.preferredWidth: 200
                onTextChanged: root.searchQuery = text
            }

        }

        ListView {
            id: packageListView

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 8
            model: {
                if (!PackageManager)
                    return [];

                // packageList プロパティへの依存関係を作成し、同期完了時に再評価を促す
                var _ = PackageManager.packageList;
                return PackageManager.searchPackages(root.searchQuery);
            }

            Label {
                anchors.centerIn: parent
                visible: packageListView.count === 0 && !PackageManager.isBusy
                text: root.searchQuery === "" ? qsTr("パッケージリストが空です。リポジトリを同期して最新情報を取得してください。") : qsTr("検索結果がありません。")
                color: palette.mid
            }

            BusyIndicator {
                anchors.centerIn: parent
                running: PackageManager && PackageManager.isBusy
                visible: running
            }

            delegate: Frame {
                readonly property string installedVer: modelData.installed_version || ""
                readonly property string latestVer: modelData.latest_version || ""
                readonly property bool hasUpdate: installedVer !== "" && latestVer !== "" && installedVer !== latestVer

                width: packageListView.width
                padding: 12

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 4

                    RowLayout {
                        Layout.fillWidth: true

                        Label {
                            text: modelData.display_name || modelData.id
                            font.bold: true
                            font.pixelSize: 14
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                        }

                        Label {
                            text: modelData.type || "unknown"
                            font.pixelSize: 10
                            color: "white"
                            padding: 4

                            background: Rectangle {
                                color: palette.highlight
                                radius: 4
                            }

                        }

                    }

                    Label {
                        text: modelData.description || ""
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        font.pixelSize: 12
                        color: palette.text
                        opacity: 0.8
                        visible: text !== ""
                    }

                    Label {
                        text: qsTr("最新バージョン: ") + (latestVer || "---")
                        font.pixelSize: 11
                        color: palette.mid
                        visible: latestVer !== ""
                    }

                    RowLayout {
                        Layout.fillWidth: true

                        Label {
                            text: modelData.author ? "Author: " + modelData.author : ""
                            font.pixelSize: 11
                            color: palette.mid
                            Layout.fillWidth: true
                            visible: modelData.author !== undefined
                        }

                        Label {
                            text: qsTr("インストール済み: ") + installedVer
                            font.pixelSize: 11
                            color: "#44cc88"
                            visible: installedVer !== "" && !hasUpdate
                        }

                        Label {
                            text: qsTr("アップデートあり: ") + latestVer
                            font.pixelSize: 11
                            color: palette.highlight
                            visible: hasUpdate
                        }

                        Button {
                            text: qsTr("削除")
                            visible: installedVer !== "" && modelData.id !== "org.aviqtl.app"
                            enabled: !PackageManager.isBusy
                            onClicked: PackageManager.removePackage(modelData.id)
                        }

                        Button {
                            text: hasUpdate ? qsTr("アップデート") : qsTr("インストール")
                            highlighted: true
                            // 同期前（latestVerが空）の場合はボタンを無効化
                            enabled: !PackageManager.isBusy && (installedVer === "" || hasUpdate) && latestVer !== ""
                            visible: installedVer === "" || hasUpdate
                            onClicked: PackageManager.fetchAssets(modelData.id)
                        }

                    }

                }

            }

        }

        ProgressBar {
            Layout.fillWidth: true
            visible: PackageManager && PackageManager.isBusy
            value: PackageManager ? PackageManager.progress : 0
        }

    }

}
