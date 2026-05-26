import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings
    required property var renderThreadValues
    required property var renderThreadLabels
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
            title: qsTr("メモリとキャッシュ")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("最大画像サイズ")
                }

                SpinBox {
                    from: 1024
                    to: 16384
                    stepSize: 512
                    value: Number(root.valueOr("maxImageSize", 8192))
                    onValueModified: root.setValue("maxImageSize", value)
                }

                Label {
                    text: qsTr("キャッシュ容量")
                }

                SpinBox {
                    from: 64
                    to: 8192
                    stepSize: 64
                    value: root.valueOr("cacheSize", 512)
                    onValueModified: root.setValue("cacheSize", value)
                }

                Label {
                    text: qsTr("描画スレッド数")
                }

                ComboBox {
                    model: renderThreadLabels
                    currentIndex: root.indexOfValue(renderThreadValues, root.valueOr("renderThreads", 0), 0)
                    onActivated: root.setValue("renderThreads", renderThreadValues[currentIndex])
                }

            }

        }

        GroupBox {
            title: qsTr("補足")
            Layout.fillWidth: true

            ColumnLayout {
                anchors.fill: parent

                Label {
                    text: qsTr("描画スレッド数が自動のときは実行環境に応じて決定します")
                    wrapMode: Text.WordWrap
                    color: root.secondaryTextColor
                }

                Label {
                    text: qsTr("ご使用の実行環境に合わせて、まずは自動設定で動作を確認してください")
                    wrapMode: Text.WordWrap
                    color: root.secondaryTextColor
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
