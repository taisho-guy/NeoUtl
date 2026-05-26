import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings
    required property var videoCodecValues
    required property var videoCodecLabels
    required property var audioCodecValues
    required property var audioCodecLabels
    required property var audioChannelValues
    required property var audioChannelLabels
    required property var blockSizeValues

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
            title: qsTr("映像")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("既定の映像コーデック")
                }

                ComboBox {
                    model: videoCodecLabels
                    currentIndex: root.indexOfValue(videoCodecValues, root.valueOr("exportDefaultCodec", "h264_vaapi"), 0)
                    onActivated: root.setValue("exportDefaultCodec", videoCodecValues[currentIndex])
                }

                Label {
                    text: qsTr("既定ビットレート")
                }

                SpinBox {
                    from: 1
                    to: 500
                    value: root.valueOr("exportDefaultBitrateMbps", 15)
                    onValueModified: root.setValue("exportDefaultBitrateMbps", value)
                }

                Label {
                    text: qsTr("既定CRF")
                }

                SpinBox {
                    from: 0
                    to: 51
                    value: root.valueOr("exportDefaultCrf", 20)
                    onValueModified: root.setValue("exportDefaultCrf", value)
                }

                Label {
                    text: qsTr("静止画品質")
                }

                SpinBox {
                    from: 0
                    to: 100
                    value: root.valueOr("exportImageQuality", 95)
                    onValueModified: root.setValue("exportImageQuality", value)
                }

                Label {
                    text: qsTr("連番桁数")
                }

                SpinBox {
                    from: 2
                    to: 10
                    value: root.valueOr("exportSequencePadding", 6)
                    onValueModified: root.setValue("exportSequencePadding", value)
                }

            }

        }

        GroupBox {
            title: qsTr("音声と進行表示")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("既定の音声コーデック")
                }

                ComboBox {
                    model: audioCodecLabels
                    currentIndex: root.indexOfValue(audioCodecValues, root.valueOr("exportDefaultAudioCodec", "aac"), 0)
                    onActivated: root.setValue("exportDefaultAudioCodec", audioCodecValues[currentIndex])
                }

                Label {
                    text: qsTr("音声ビットレート")
                }

                SpinBox {
                    from: 32
                    to: 1536
                    stepSize: 32
                    value: root.valueOr("exportDefaultAudioBitrateKbps", 192)
                    onValueModified: root.setValue("exportDefaultAudioBitrateKbps", value)
                }

                Label {
                    text: qsTr("フレーム取得待ち時間")
                }

                SpinBox {
                    from: 100
                    to: 10000
                    stepSize: 100
                    value: root.valueOr("exportFrameGrabTimeoutMs", 2000)
                    onValueModified: root.setValue("exportFrameGrabTimeoutMs", value)
                }

                Label {
                    text: qsTr("進捗更新間隔")
                }

                SpinBox {
                    from: 1
                    to: 60
                    value: root.valueOr("exportProgressInterval", 5)
                    onValueModified: root.setValue("exportProgressInterval", value)
                }

            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
