import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Dialog {
    id: root
    title: qsTr("システム設定")
    modal: true
    focus: true
    standardButtons: Dialog.Ok | Dialog.Cancel
    width: 480
    height: 380
    anchors.centerIn: parent

    background: Rectangle {
        color: "#1e2230"
        border.color: "#32384e"
        radius: 8
    }

    property int workerThreads: 4
    property bool crashReportingEnabled: false
    property bool autoAudioScan: true

    contentItem: ColumnLayout {
        spacing: 16

        GridLayout {
            columns: 2
            rowSpacing: 14
            columnSpacing: 16
            Layout.fillWidth: true

            Label { text: qsTr("デコード並行スレッド数:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            SpinBox {
                from: 1; to: 64; value: root.workerThreads
                editable: true
                Layout.fillWidth: true
                onValueModified: root.workerThreads = value
            }

            Label { text: qsTr("クラッシュレポート送信:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            CheckBox {
                text: qsTr("Sentryに診断情報を送信する")
                checked: root.crashReportingEnabled
                onToggled: root.crashReportingEnabled = checked
            }

            Label { text: qsTr("オーディオプラグイン自動検出:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            CheckBox {
                text: qsTr("起動時にVST3 / LV2 / CLAPを再スキャン")
                checked: root.autoAudioScan
                onToggled: root.autoAudioScan = checked
            }

            Label { text: qsTr("レンダリングAPI:"); color: "#c8d0e0"; Layout.alignment: Qt.AlignRight }
            Label {
                text: "Qt RHI (Vulkan / Metal / DX12) + wgpu Zero-Copy"
                color: "#6ce48e"
                font.bold: true
            }
        }

        Item { Layout.fillHeight: true }
    }
}
