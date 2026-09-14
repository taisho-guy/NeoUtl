import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Dialog {
    id: root
    title: qsTr("プロジェクト設定")
    modal: true
    focus: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    width: 440
    height: 380
    anchors.centerIn: parent

    background: Rectangle {
        color: "#1e2230"
        border.color: "#32384e"
        radius: 8
    }

    property int projWidth: 1920
    property int projHeight: 1080
    property int projFps: 60
    property int projAudioRate: 48000
    property int projTotalFrames: 1800

    contentItem: ColumnLayout {
        spacing: 16

        GridLayout {
            columns: 2
            rowSpacing: 12
            columnSpacing: 16
            Layout.fillWidth: true

            Label { text: qsTr("横幅 (px):"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            SpinBox {
                from: 16; to: 7680; stepSize: 16; value: root.projWidth
                editable: true
                Layout.fillWidth: true
                onValueModified: root.projWidth = value
            }

            Label { text: qsTr("縦幅 (px):"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            SpinBox {
                from: 16; to: 4320; stepSize: 16; value: root.projHeight
                editable: true
                Layout.fillWidth: true
                onValueModified: root.projHeight = value
            }

            Label { text: qsTr("フレームレート (fps):"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            ComboBox {
                model: [24, 30, 60, 120]
                currentIndex: 2
                Layout.fillWidth: true
                onActivated: root.projFps = currentText
            }

            Label { text: qsTr("総フレーム数:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            SpinBox {
                from: 1; to: 1000000; stepSize: 30; value: root.projTotalFrames
                editable: true
                Layout.fillWidth: true
                onValueModified: root.projTotalFrames = value
            }

            Label { text: qsTr("音声サンプリング周波数:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            ComboBox {
                model: ["44100 Hz", "48000 Hz", "96000 Hz"]
                currentIndex: 1
                Layout.fillWidth: true
                onActivated: {
                    if (index === 0) root.projAudioRate = 44100;
                    else if (index === 1) root.projAudioRate = 48000;
                    else root.projAudioRate = 96000;
                }
            }
        }

        Item { Layout.fillHeight: true }
    }
}
