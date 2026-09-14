import QtQuick
import QtQuick.Controls
import ".."

Row {
    id: control

    property alias value: spin.value
    property alias from: spin.from
    property alias to: spin.to
    property alias stepSize: spin.stepSize
    property string suffix: ""
    property string label: ""

    spacing: 6

    Text {
        visible: control.label.length > 0
        text: control.label
        color: Theme.textMuted
        font.family: Theme.fontFamily
        font.pixelSize: 12
        anchors.verticalCenter: parent.verticalCenter
    }

    SpinBox {
        id: spin
        from: 0
        to: 999999
        stepSize: 1
        editable: true
        implicitHeight: 28
        implicitWidth: 100

        contentItem: TextInput {
            text: spin.textFromValue(spin.value, spin.locale) + (control.suffix ? " " + control.suffix : "")
            font.family: Theme.monoFont
            font.pixelSize: 12
            color: Theme.text
            horizontalAlignment: Qt.AlignHCenter
            verticalAlignment: Qt.AlignVCenter
            readOnly: !spin.editable
            validator: spin.validator
            inputMethodHints: Qt.ImhFormattedNumbersOnly
        }

        up.indicator: Rectangle {
            x: spin.mirrored ? 0 : spin.width - width
            height: spin.height
            implicitWidth: 20
            color: spin.up.pressed ? Theme.cardHover : (spin.up.hovered ? Qt.rgba(255,255,255,0.08) : "transparent")
            Text {
                text: "+"
                font.pixelSize: 12
                color: Theme.textMuted
                anchors.centerIn: parent
            }
        }

        down.indicator: Rectangle {
            x: spin.mirrored ? spin.width - width : 0
            height: spin.height
            implicitWidth: 20
            color: spin.down.pressed ? Theme.cardHover : (spin.down.hovered ? Qt.rgba(255,255,255,0.08) : "transparent")
            Text {
                text: "-"
                font.pixelSize: 12
                color: Theme.textMuted
                anchors.centerIn: parent
            }
        }

        background: Rectangle {
            radius: Theme.radiusSm
            color: Theme.bgDark
            border.color: spin.activeFocus ? Theme.accent : Theme.border
            border.width: 1
        }
    }
}
