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
            title: qsTr("既定値")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("幅")
                }

                SpinBox {
                    from: 1
                    to: 16000
                    value: root.valueOr("defaultProjectWidth", 1920)
                    onValueModified: root.setValue("defaultProjectWidth", value)
                }

                Label {
                    text: qsTr("高さ")
                }

                SpinBox {
                    from: 1
                    to: 16000
                    value: root.valueOr("defaultProjectHeight", 1080)
                    onValueModified: root.setValue("defaultProjectHeight", value)
                }

                Label {
                    text: qsTr("フレームレート")
                }

                SpinBox {
                    from: 100
                    to: 24000
                    stepSize: 100
                    value: Math.round(root.valueOr("defaultProjectFps", 60) * 100)
                    textFromValue: function(value, locale) {
                        return (value / 100).toFixed(2);
                    }
                    valueFromText: function(text, locale) {
                        return Math.round(Number(text) * 100);
                    }
                    onValueModified: root.setValue("defaultProjectFps", value / 100)
                }

                Label {
                    text: qsTr("総フレーム数")
                }

                SpinBox {
                    from: 1
                    to: 1e+06
                    value: root.valueOr("defaultProjectFrames", 3600)
                    onValueModified: root.setValue("defaultProjectFrames", value)
                }

                Label {
                    text: qsTr("サンプリング周波数")
                }

                SpinBox {
                    from: 8000
                    to: 192000
                    stepSize: 1000
                    value: root.valueOr("defaultProjectSampleRate", 48000)
                    onValueModified: root.setValue("defaultProjectSampleRate", value)
                }

                Label {
                    text: qsTr("既定クリップ長")
                }

                SpinBox {
                    from: 1
                    to: 100000
                    value: root.valueOr("defaultClipDuration", 100)
                    onValueModified: root.setValue("defaultClipDuration", value)
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
