import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15

Rectangle {
    id: root
    color: "#181b26"
    border.color: "#272d40"
    border.width: 1

    property int clipId: -1
    property string clipName: qsTr("未選択")
    property string clipKind: ""
    property real posX: 0.0
    property real posY: 0.0
    property real posZ: 0.0
    property real scaleVal: 100.0
    property real rotationVal: 0.0
    property real opacityVal: 100.0
    property var activeEffects: []

    signal addEffectRequested()

    ScrollView {
        anchors.fill: parent
        anchors.margins: 12
        clip: true

        ColumnLayout {
            width: root.width - 24
            spacing: 14

                        RowLayout {
                Layout.fillWidth: true
                Label {
                    text: qsTr("オブジェクト設定")
                    font.bold: true
                    font.pixelSize: 14
                    color: "#f1f5f9"
                }
                Item { Layout.fillWidth: true }
                Button {
                    text: qsTr("+ エフェクト追加")
                    enabled: root.clipId !== -1
                    implicitHeight: 28
                    onClicked: root.addEffectRequested()
                }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: "#272d40"
            }

                        Rectangle {
                Layout.fillWidth: true
                height: 48
                color: "#1e2233"
                radius: 6
                border.color: "#30374e"
                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 8
                    Rectangle {
                        width: 32; height: 32; radius: 4
                        color: root.clipId !== -1 ? "#3b82f6" : "#475569"
                        Label {
                            anchors.centerIn: parent
                            text: root.clipKind === "text" ? "T" : (root.clipKind === "audio" ? "♪" : "■")
                            color: "white"
                            font.bold: true
                        }
                    }
                    ColumnLayout {
                        spacing: 2
                        Label { text: root.clipName; color: "white"; font.bold: true; font.pixelSize: 12 }
                        Label { text: root.clipId !== -1 ? ("ID: " + root.clipId + " (" + root.clipKind + ")") : qsTr("クリップを選択してください"); color: "#94a3b8"; font.pixelSize: 10 }
                    }
                }
            }

                        Label {
                text: qsTr("基本座標 / トランスフォーム")
                font.bold: true
                font.pixelSize: 12
                color: "#93c5fd"
            }

            GridLayout {
                columns: 2
                rowSpacing: 8
                columnSpacing: 12
                Layout.fillWidth: true

                Label { text: "X:"; color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: -1920; to: 1920; value: root.posX
                        onMoved: root.posX = value
                    }
                    SpinBox {
                        from: -1920; to: 1920; value: Math.round(root.posX); editable: true
                        onValueModified: root.posX = value
                    }
                }

                Label { text: "Y:"; color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: -1080; to: 1080; value: root.posY
                        onMoved: root.posY = value
                    }
                    SpinBox {
                        from: -1080; to: 1080; value: Math.round(root.posY); editable: true
                        onValueModified: root.posY = value
                    }
                }

                Label { text: "Z:"; color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: -2000; to: 2000; value: root.posZ
                        onMoved: root.posZ = value
                    }
                    SpinBox {
                        from: -2000; to: 2000; value: Math.round(root.posZ); editable: true
                        onValueModified: root.posZ = value
                    }
                }

                Label { text: qsTr("拡大率 (%):"); color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: 0; to: 500; value: root.scaleVal
                        onMoved: root.scaleVal = value
                    }
                    SpinBox {
                        from: 0; to: 500; value: Math.round(root.scaleVal); editable: true
                        onValueModified: root.scaleVal = value
                    }
                }

                Label { text: qsTr("回転 (度):"); color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: -360; to: 360; value: root.rotationVal
                        onMoved: root.rotationVal = value
                    }
                    SpinBox {
                        from: -360; to: 360; value: Math.round(root.rotationVal); editable: true
                        onValueModified: root.rotationVal = value
                    }
                }

                Label { text: qsTr("透明度 (%):"); color: "#cbd5e1"; Layout.alignment: Qt.AlignRight }
                RowLayout {
                    Layout.fillWidth: true
                    Slider {
                        Layout.fillWidth: true; from: 0; to: 100; value: root.opacityVal
                        onMoved: root.opacityVal = value
                    }
                    SpinBox {
                        from: 0; to: 100; value: Math.round(root.opacityVal); editable: true
                        onValueModified: root.opacityVal = value
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: "#272d40"
            }

                        Label {
                text: qsTr("適用済みエフェクト")
                font.bold: true
                font.pixelSize: 12
                color: "#f472b6"
            }

            Repeater {
                model: root.activeEffects
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: 72
                    color: "#1e2233"
                    radius: 6
                    border.color: "#30374e"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 8
                        RowLayout {
                            Layout.fillWidth: true
                            Label { text: modelData.name; color: "white"; font.bold: true }
                            Item { Layout.fillWidth: true }
                            Button { text: "✕"; implicitWidth: 24; implicitHeight: 24 }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            Label { text: qsTr("強度:"); color: "#cbd5e1"; font.pixelSize: 11 }
                            Slider { Layout.fillWidth: true; from: 0; to: 100; value: 50 }
                        }
                    }
                }
            }

            Item { Layout.fillHeight: true }
        }
    }
}
