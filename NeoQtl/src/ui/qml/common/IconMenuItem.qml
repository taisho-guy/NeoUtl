import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

MenuItem {
    id: control

    property string iconName: ""

            indicator: Item {
    }

    contentItem: RowLayout {
        spacing: 6

        AviQtlIcon {
            visible: control.checkable
            iconName: "check_line"
            size: 18
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
            opacity: control.checked ? 1 : 0
            Layout.alignment: Qt.AlignVCenter
        }

                AviQtlIcon {
            iconName: control.iconName
            size: 18
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
            visible: control.iconName !== ""
            Layout.alignment: Qt.AlignVCenter
            Layout.rightMargin: 4
        }

                Text {
            text: control.text
            font: control.font
            opacity: enabled ? 1 : 0.3
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
            horizontalAlignment: Text.AlignLeft
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            Layout.fillWidth: true
        }

                Text {
            text: control.action ? (control.action.shortcutText !== undefined ? control.action.shortcutText : control.action.shortcut) : ""
            font: control.font
            opacity: enabled ? 1 : 0.3
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
            horizontalAlignment: Text.AlignRight
            verticalAlignment: Text.AlignVCenter
            visible: text !== ""
            Layout.leftMargin: 12
        }

    }

}
