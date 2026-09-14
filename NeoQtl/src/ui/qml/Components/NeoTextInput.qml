import QtQuick
import QtQuick.Controls
import ".."

TextField {
    id: control

    implicitHeight: 28
    font.family: Theme.fontFamily
    font.pixelSize: 13
    color: Theme.text
    placeholderTextColor: Theme.textMuted
    selectByMouse: true
    verticalAlignment: TextInput.AlignVCenter

    background: Rectangle {
        radius: Theme.radiusSm
        color: Theme.bgDark
        border.color: control.activeFocus ? Theme.accent :
                      control.hovered ? Theme.textMuted : Theme.border
        border.width: 1
    }
}
