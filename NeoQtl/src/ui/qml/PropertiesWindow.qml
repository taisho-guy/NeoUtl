import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "./Components"

Window {
    id: propertiesWindow
    width: 720
    height: 540
    minimumWidth: 480
    minimumHeight: 320
    title: "NeoUtl - オブジェクト設定"
    color: Theme.bg

    property int targetObjectId: -1
    property int currentFrame: 0

    signal openEffectAddRequested()
    signal openEasingEditorRequested(string paramKey)
    signal paramChanged(string key, real value)
    signal effectToggled(int effectIndex, bool enabled)
    signal effectRemoved(int effectIndex)

        property var appliedEffects: [
        { name: "ColorCorrection", enabled: true },
        { name: "DropShadow", enabled: true }
    ]

        property real posX: 0.0
    property real posY: 0.0
    property real posZ: 0.0
    property real scaleX: 100.0
    property real scaleY: 100.0
    property real rotationZ: 0.0
    property real opacityVal: 100.0

    RowLayout {
        anchors.fill: parent
        spacing: 0

                Rectangle {
            Layout.preferredWidth: 180
            Layout.fillHeight: true
            color: Theme.bgDark
            border.color: Theme.border
            border.width: 1

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    Text {
                        text: "エフェクト"
                        color: Theme.accent
                        font.family: Theme.fontFamily
                        font.pixelSize: 13
                        font.bold: true
                    }
                    Item { Layout.fillWidth: true }
                    NeoButton {
                        text: "＋追加"
                        outline: true
                        implicitHeight: 22
                        implicitWidth: 48
                        onClicked: propertiesWindow.openEffectAddRequested()
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 1
                    color: Theme.border
                }

                ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true

                    ListView {
                        width: parent.width
                        spacing: 4
                        model: propertiesWindow.appliedEffects

                        delegate: Rectangle {
                            width: parent.width
                            height: 32
                            radius: Theme.radiusSm
                            color: Theme.card
                            border.color: Theme.borderMuted

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 6
                                anchors.rightMargin: 6
                                spacing: 6

                                CheckBox {
                                    checked: modelData.enabled
                                    onCheckedChanged: propertiesWindow.effectToggled(index, checked)
                                }

                                Text {
                                    Layout.fillWidth: true
                                    text: modelData.name
                                    color: Theme.text
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    elide: Text.ElideRight
                                }

                                NeoButton {
                                    text: "×"
                                    outline: true
                                    danger: true
                                    implicitHeight: 20
                                    implicitWidth: 20
                                    onClicked: propertiesWindow.effectRemoved(index)
                                }
                            }
                        }
                    }
                }
            }
        }

                ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ColumnLayout {
                width: parent.width
                spacing: 12

                Item { height: 4 }

                                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    Layout.rightMargin: 16
                    Text {
                        text: "プロパティ"
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: 16
                        font.bold: true
                    }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: "Object " + (propertiesWindow.targetObjectId >= 0 ? propertiesWindow.targetObjectId : "-") + " / frame " + propertiesWindow.currentFrame
                        color: Theme.textMuted
                        font.family: Theme.monoFont
                        font.pixelSize: 12
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    Layout.rightMargin: 16
                    height: 1
                    color: Theme.border
                }

                                NeoCard {
                    Layout.fillWidth: true
                    Layout.leftMargin: 16
                    Layout.rightMargin: 16
                    heading: "トランスフォーム"

                    ColumnLayout {
                        width: parent.width
                        spacing: 8

                                                RowLayout {
                            width: parent.width
                            Text { text: "X"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 60 }
                            Slider {
                                id: sliderX
                                Layout.fillWidth: true
                                from: -1920
                                to: 1920
                                value: propertiesWindow.posX
                                onValueChanged: {
                                    propertiesWindow.posX = value;
                                    propertiesWindow.paramChanged("x", value);
                                }
                            }
                            Text {
                                text: Math.round(sliderX.value).toString()
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                Layout.preferredWidth: 40
                            }
                            NeoButton {
                                text: "曲線"
                                outline: true
                                implicitHeight: 22
                                implicitWidth: 36
                                onClicked: propertiesWindow.openEasingEditorRequested("x")
                            }
                        }

                                                RowLayout {
                            width: parent.width
                            Text { text: "Y"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 60 }
                            Slider {
                                id: sliderY
                                Layout.fillWidth: true
                                from: -1080
                                to: 1080
                                value: propertiesWindow.posY
                                onValueChanged: {
                                    propertiesWindow.posY = value;
                                    propertiesWindow.paramChanged("y", value);
                                }
                            }
                            Text {
                                text: Math.round(sliderY.value).toString()
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                Layout.preferredWidth: 40
                            }
                            NeoButton {
                                text: "曲線"
                                outline: true
                                implicitHeight: 22
                                implicitWidth: 36
                                onClicked: propertiesWindow.openEasingEditorRequested("y")
                            }
                        }

                                                RowLayout {
                            width: parent.width
                            Text { text: "拡大率"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 60 }
                            Slider {
                                id: sliderScale
                                Layout.fillWidth: true
                                from: 0
                                to: 500
                                value: propertiesWindow.scaleX
                                onValueChanged: {
                                    propertiesWindow.scaleX = value;
                                    propertiesWindow.scaleY = value;
                                    propertiesWindow.paramChanged("scale_x", value);
                                    propertiesWindow.paramChanged("scale_y", value);
                                }
                            }
                            Text {
                                text: Math.round(sliderScale.value) + "%"
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                Layout.preferredWidth: 40
                            }
                            NeoButton {
                                text: "曲線"
                                outline: true
                                implicitHeight: 22
                                implicitWidth: 36
                                onClicked: propertiesWindow.openEasingEditorRequested("scale_x")
                            }
                        }

                                                RowLayout {
                            width: parent.width
                            Text { text: "回転"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 60 }
                            Slider {
                                id: sliderRot
                                Layout.fillWidth: true
                                from: -360
                                to: 360
                                value: propertiesWindow.rotationZ
                                onValueChanged: {
                                    propertiesWindow.rotationZ = value;
                                    propertiesWindow.paramChanged("rotation_z", value);
                                }
                            }
                            Text {
                                text: Math.round(sliderRot.value) + "°"
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                Layout.preferredWidth: 40
                            }
                            NeoButton {
                                text: "曲線"
                                outline: true
                                implicitHeight: 22
                                implicitWidth: 36
                                onClicked: propertiesWindow.openEasingEditorRequested("rotation_z")
                            }
                        }

                                                RowLayout {
                            width: parent.width
                            Text { text: "透明度"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.preferredWidth: 60 }
                            Slider {
                                id: sliderOpacity
                                Layout.fillWidth: true
                                from: 0
                                to: 100
                                value: propertiesWindow.opacityVal
                                onValueChanged: {
                                    propertiesWindow.opacityVal = value;
                                    propertiesWindow.paramChanged("opacity", value);
                                }
                            }
                            Text {
                                text: Math.round(sliderOpacity.value) + "%"
                                color: Theme.textMuted
                                font.family: Theme.monoFont
                                font.pixelSize: 12
                                Layout.preferredWidth: 40
                            }
                            NeoButton {
                                text: "曲線"
                                outline: true
                                implicitHeight: 22
                                implicitWidth: 36
                                onClicked: propertiesWindow.openEasingEditorRequested("opacity")
                            }
                        }
                    }
                }

                Item { height: 16 }
            }
        }
    }
}
