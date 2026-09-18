import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings

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
            title: qsTr("表示")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("文字余白係数")
                }

                SpinBox {
                    from: 1
                    to: 20
                    stepSize: 1
                    value: Math.round(root.valueOr("textPaddingMultiplier", 4) * 10)
                    textFromValue: function(value, locale) {
                        return (value / 10).toFixed(1);
                    }
                    valueFromText: function(text, locale) {
                        return Math.round(Number(text) * 10);
                    }
                    onValueModified: root.setValue("textPaddingMultiplier", value / 10)
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
