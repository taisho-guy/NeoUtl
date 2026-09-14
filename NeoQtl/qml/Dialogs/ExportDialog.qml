import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Dialog {
    id: root
    title: qsTr("動画 / 音声エクスポート")
    modal: true
    focus: true
    standardButtons: Dialog.Close
    width: 520
    height: 400
    anchors.centerIn: parent

    background: Rectangle {
        color: "#1e2230"
        border.color: "#32384e"
        radius: 8
    }

    property string outputPath: "output.mp4"
    property bool isExporting: false
    property real exportProgress: 0.0
    property string exportStatus: qsTr("待機中")

    signal startExportRequested(string path, string format)

    contentItem: ColumnLayout {
        spacing: 14

        Label {
            text: qsTr("エクスポート設定")
            font.bold: true
            font.pixelSize: 14
            color: "#6c8cff"
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            TextField {
                id: pathInput
                Layout.fillWidth: true
                text: root.outputPath
                placeholderText: qsTr("出力ファイルパス...")
                color: "white"
                background: Rectangle {
                    color: "#141722"
                    border.color: pathInput.activeFocus ? "#5e81ff" : "#2b3145"
                    radius: 4
                }
                onTextChanged: root.outputPath = text
            }
            Button {
                text: qsTr("参照...")
                onClicked: {
                                        root.outputPath = "export_" + Date.now() + ".mp4";
                }
            }
        }

        GridLayout {
            columns: 2
            rowSpacing: 10
            columnSpacing: 12
            Layout.fillWidth: true

            Label { text: qsTr("フォーマット:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            ComboBox {
                id: formatCombo
                model: ["MP4 (H.264 / AAC)", "MP4 (HEVC / AAC)", "WebM (VP9 / Opus)", "WAV (非圧縮音声)"]
                Layout.fillWidth: true
            }

            Label { text: qsTr("品質 / レート制御:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            ComboBox {
                model: [qsTr("高品質 (CRF 18)"), qsTr("標準 (CRF 23)"), qsTr("軽量 (CRF 28)")]
                currentIndex: 1
                Layout.fillWidth: true
            }

            Label { text: qsTr("ハードウェア加速:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            CheckBox {
                text: qsTr("自動検出 (NVENC / QSV / VAAPI / AMF)")
                checked: true
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: "#2a3044"
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6

            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("状態: ") + root.exportStatus; color: "#a0aac0" }
                Item { Layout.fillWidth: true }
                Label { text: Math.round(root.exportProgress * 100) + "%"; color: "#8ab4f8"; font.bold: true }
            }

            ProgressBar {
                Layout.fillWidth: true
                value: root.exportProgress
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            Button {
                text: root.isExporting ? qsTr("中止") : qsTr("エクスポート開始")
                highlighted: !root.isExporting
                onClicked: {
                    if (root.isExporting) {
                        root.isExporting = false;
                        root.exportStatus = qsTr("中断されました");
                    } else {
                        root.isExporting = true;
                        root.exportProgress = 0.0;
                        root.exportStatus = qsTr("レンダリングおよびエンコード中...");
                        root.startExportRequested(root.outputPath, formatCombo.currentText);
                    }
                }
            }
        }

        Timer {
            id: simTimer
            interval: 100
            running: root.isExporting && root.exportProgress < 1.0
            repeat: true
            onTriggered: {
                root.exportProgress = Math.min(1.0, root.exportProgress + 0.02);
                if (root.exportProgress >= 1.0) {
                    root.isExporting = false;
                    root.exportStatus = qsTr("完了しました！");
                }
            }
        }
    }
}
