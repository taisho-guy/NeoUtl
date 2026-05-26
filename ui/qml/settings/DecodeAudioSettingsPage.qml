import "../common" as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ScrollView {
    id: root

    required property var draftSettings
    required property var audioChannelValues
    required property var audioChannelLabels
    required property var blockSizeValues
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
            title: qsTr("映像デコード")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("インデックス予約数")
                }

                SpinBox {
                    from: 1000
                    to: 1e+06
                    stepSize: 1000
                    value: root.valueOr("videoDecoderIndexReserve", 108000)
                    onValueModified: root.setValue("videoDecoderIndexReserve", value)
                }

                Label {
                    text: qsTr("最小キャッシュ量")
                }

                SpinBox {
                    from: 16
                    to: 4096
                    stepSize: 16
                    value: root.valueOr("videoDecoderMinCacheMB", 64)
                    onValueModified: root.setValue("videoDecoderMinCacheMB", value)
                }

                Label {
                    text: qsTr("ハードウェアフレームプール数")
                }

                SpinBox {
                    from: 1
                    to: 256
                    value: root.valueOr("hwFramePoolSize", 32)
                    onValueModified: root.setValue("hwFramePoolSize", value)
                }

            }

        }

        GroupBox {
            title: qsTr("音声")
            Layout.fillWidth: true

            GridLayout {
                columns: 2
                columnSpacing: 12
                rowSpacing: 8
                anchors.fill: parent

                Label {
                    text: qsTr("音声チャンネル数")
                }

                ComboBox {
                    model: audioChannelLabels
                    currentIndex: root.indexOfValue(audioChannelValues, root.valueOr("audioChannels", 2), 1)
                    onActivated: root.setValue("audioChannels", audioChannelValues[currentIndex])
                }

                Label {
                    text: qsTr("プラグイン最大ブロックサイズ")
                }

                ComboBox {
                    model: blockSizeValues
                    currentIndex: root.indexOfValue(blockSizeValues, root.valueOr("audioPluginMaxBlockSize", 4096), 4)
                    onActivated: root.setValue("audioPluginMaxBlockSize", blockSizeValues[currentIndex])
                }

                Label {
                    text: qsTr("Lua フック間隔")
                }

                SpinBox {
                    from: 1
                    to: 1000
                    stepSize: 1
                    value: root.valueOr("luaHookIntervalMs", 16)
                    onValueModified: root.setValue("luaHookIntervalMs", value)
                }

            }

        }

        Label {
            text: qsTr("デコードと音声関連の設定は再起動後に反映されます")
            color: root.secondaryTextColor
            wrapMode: Text.WordWrap
        }

        Item {
            Layout.fillHeight: true
        }

    }

}
