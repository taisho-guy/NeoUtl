import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings
    readonly property color secondaryTextColor: Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.7)

    signal valueChanged(string key, var value)

    function setValue(key, value) {
        valueChanged(key, value);
    }

    function valueOr(key, fb) {
        return draftSettings[key] !== undefined ? draftSettings[key] : fb;
    }

    function indexOfValue(values, target, fallback) {
        for (var i = 0; i < values.length; ++i) if (values[i] === target) {
            return i;
        }
        return fallback;
    }

    Layout.fillWidth: true
    Layout.fillHeight: true
    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        width: root.availableWidth
        spacing: 14

        GroupBox {
            title: qsTr("ファイル")
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent

                CheckBox {
                    text: qsTr("終了時に確認ダイアログを表示する")
                    checked: root.valueOr("showConfirmOnClose", true)
                    onToggled: root.setValue("showConfirmOnClose", checked)
                }

                CheckBox {
                    text: qsTr("自動バックアップを有効にする")
                    checked: root.valueOr("enableAutoBackup", true)
                    onToggled: root.setValue("enableAutoBackup", checked)
                }

                RowLayout {
                    enabled: root.valueOr("enableAutoBackup", true)

                    Label {
                        text: qsTr("バックアップ間隔")
                    }

                    SpinBox {
                        from: 1
                        to: 60
                        value: root.valueOr("backupInterval", 5)
                        onValueModified: root.setValue("backupInterval", value)
                    }

                    Label {
                        text: qsTr("分")
                    }

                }

                RowLayout {
                    Label {
                        text: qsTr("最近使ったプロジェクトの保持数")
                    }

                    SpinBox {
                        from: 1
                        to: 50
                        value: root.valueOr("recentProjectMaxCount", 10)
                        onValueModified: root.setValue("recentProjectMaxCount", value)
                    }

                    Label {
                        text: qsTr("件")
                    }

                }

            }

        }

        GroupBox {
            title: qsTr("編集")
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent

                RowLayout {
                    Label {
                        text: qsTr("元に戻す回数")
                    }

                    SpinBox {
                        from: 1
                        to: 1000
                        value: root.valueOr("undoCount", 32)
                        onValueModified: root.setValue("undoCount", value)
                    }

                }

                Label {
                    text: qsTr("回数を増やすとメモリ使用量が増えます")
                    color: root.secondaryTextColor
                    font.pixelSize: 11
                }

            }

        }

        GroupBox {
            title: qsTr("起動")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("スプラッシュ表示時間")
                }

                SpinBox {
                    from: 0
                    to: 10000
                    stepSize: 100
                    value: root.valueOr("splashDuration", 512)
                    onValueModified: root.setValue("splashDuration", value)
                }

                Label {
                    text: qsTr("スプラッシュ画像サイズ")
                }

                SpinBox {
                    from: 128
                    to: 2048
                    stepSize: 64
                    value: root.valueOr("splashSize", 128)
                    onValueModified: root.setValue("splashSize", value)
                }

                Label {
                    text: qsTr("起動後の遅延時間")
                }

                SpinBox {
                    from: 0
                    to: 10000
                    stepSize: 100
                    value: root.valueOr("appStartupDelay", 1000)
                    onValueModified: root.setValue("appStartupDelay", value)
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
