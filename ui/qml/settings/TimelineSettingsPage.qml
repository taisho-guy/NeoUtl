import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings
    required property var timeUnitValues
    required property var timeUnitLabels

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
            title: qsTr("操作")
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent

                RowLayout {
                    Label {
                        text: qsTr("時間表示")
                    }

                    ComboBox {
                        model: timeUnitLabels
                        currentIndex: root.indexOfValue(timeUnitValues, root.valueOr("timeUnit", "frame"), 0)
                        onActivated: root.setValue("timeUnit", timeUnitValues[currentIndex])
                    }

                }

                CheckBox {
                    text: qsTr("分割時にカーソル位置を使う")
                    checked: root.valueOr("splitAtCursor", true)
                    onToggled: root.setValue("splitAtCursor", checked)
                }

                CheckBox {
                    text: qsTr("レイヤー範囲を表示する")
                    checked: root.valueOr("showLayerRange", true)
                    onToggled: root.setValue("showLayerRange", checked)
                }

            }

        }

        GroupBox {
            title: qsTr("見た目と寸法")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("トラックの高さ")
                }

                SpinBox {
                    from: 16
                    to: 100
                    value: root.valueOr("timelineTrackHeight", 30)
                    onValueModified: root.setValue("timelineTrackHeight", value)
                }

                Label {
                    text: qsTr("ヘッダーの高さ")
                }

                SpinBox {
                    from: 16
                    to: 100
                    value: root.valueOr("timelineHeaderHeight", 28)
                    onValueModified: root.setValue("timelineHeaderHeight", value)
                }

                Label {
                    text: qsTr("設定サイドバーを右に配置")
                }

                CheckBox {
                    checked: root.valueOr("settingDialogSidebarRight", false)
                    onToggled: root.setValue("settingDialogSidebarRight", checked)
                }

                Label {
                    text: qsTr("ルーラーの高さ")
                }

                SpinBox {
                    from: 16
                    to: 100
                    value: root.valueOr("timelineRulerHeight", 32)
                    onValueModified: root.setValue("timelineRulerHeight", value)
                }

                Label {
                    text: qsTr("最大レイヤー数")
                }

                SpinBox {
                    from: 1
                    to: 512
                    value: root.valueOr("timelineMaxLayers", 128)
                    onValueModified: root.setValue("timelineMaxLayers", value)
                }

                Label {
                    text: qsTr("レイヤーヘッダー幅")
                }

                SpinBox {
                    from: 40
                    to: 300
                    value: root.valueOr("timelineLayerHeaderWidth", 60)
                    onValueModified: root.setValue("timelineLayerHeaderWidth", value)
                }

                Label {
                    text: qsTr("時間表示欄の幅")
                }

                SpinBox {
                    from: 40
                    to: 300
                    value: root.valueOr("timelineRulerTimeWidth", 70)
                    onValueModified: root.setValue("timelineRulerTimeWidth", value)
                }

                Label {
                    text: qsTr("クリップ端のつかみ幅")
                }

                SpinBox {
                    from: 4
                    to: 40
                    value: root.valueOr("timelineClipResizeHandleWidth", 10)
                    onValueModified: root.setValue("timelineClipResizeHandleWidth", value)
                }

            }

        }

        GroupBox {
            title: qsTr("編集制約とズーム")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("最小クリップ長")
                }

                SpinBox {
                    from: 1
                    to: 100
                    value: root.valueOr("minClipDurationFrames", 5)
                    onValueModified: root.setValue("minClipDurationFrames", value)
                }

                Label {
                    text: qsTr("ズーム最小値")
                }

                SpinBox {
                    from: 1
                    to: 1000
                    value: root.valueOr("timelineZoomMin", 10)
                    onValueModified: root.setValue("timelineZoomMin", value)
                }

                Label {
                    text: qsTr("ズーム最大値")
                }

                SpinBox {
                    from: 1
                    to: 4000
                    value: root.valueOr("timelineZoomMax", 400)
                    onValueModified: root.setValue("timelineZoomMax", value)
                }

                Label {
                    text: qsTr("ズーム刻み")
                }

                SpinBox {
                    from: 1
                    to: 100
                    value: root.valueOr("timelineZoomStep", 10)
                    onValueModified: root.setValue("timelineZoomStep", value)
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
