import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog
    width: 620
    height: 560
    title: "メディアの書き出し"
    color: Theme.bg
    modality: Qt.WindowModal

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        NeoCard {
            Layout.fillWidth: true
            heading: "書き出しプリセット"

            RowLayout {
                width: parent.width
                spacing: 8
                NeoTextInput {
                    id: presetNameInput
                    Layout.fillWidth: true
                    text: "YouTube 1080p 60fps"
                }
                NeoButton { text: "保存" }
                NeoButton { outline: true; danger: true; text: "削除" }
            }
        }

        NeoCard {
            Layout.fillWidth: true
            heading: "出力ファイル"

            RowLayout {
                width: parent.width
                spacing: 8
                NeoTextInput {
                    id: outputPathInput
                    Layout.fillWidth: true
                    placeholderText: "保存先パスを選択してください…"
                }
                NeoButton {
                    outline: true
                    text: "参照…"
                }
            }
        }

        NeoCard {
            Layout.fillWidth: true
            heading: "エンコード設定"

            RowLayout {
                spacing: 16
                ColumnLayout {
                    Text { text: "コーデック"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                    ComboBox {
                        model: ["H.264 / AVC", "H.265 / HEVC"]
                    }
                }
                ColumnLayout {
                    Text { text: "バックエンド"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                    ComboBox {
                        model: ["自動検出", "GpuVideo (ハードウェア)", "GStreamer (システム)"]
                    }
                }
            }

            RowLayout {
                spacing: 16
                NeoNumberInput { label: "平均ビットレート:"; value: 8000; from: 500; to: 100000; suffix: "kbps" }
                NeoNumberInput { label: "最大ビットレート:"; value: 12000; from: 500; to: 150000; suffix: "kbps" }
            }
        }

        Item { Layout.fillHeight: true }

                RowLayout {
            Layout.fillWidth: true
            spacing: 8
            Item { Layout.fillWidth: true }
            NeoButton { outline: true; text: "閉じる"; onClicked: dialog.close() }
            NeoButton { text: "書き出し開始" }
        }
    }
}
