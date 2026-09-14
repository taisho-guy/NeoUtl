import QtQuick
import QtQuick.Controls
import ".."

Button {
    id: control

    property bool outline: false
    property bool danger: false
    property color customColor: "transparent"

    implicitHeight: 28
    implicitWidth: Math.max(60, contentItem.implicitWidth + 16)

    contentItem: Text {
        text: control.text
        font.family: Theme.fontFamily
        font.pixelSize: 13
        color: !control.enabled ? Theme.textMuted :
               (control.danger ? Theme.danger :
               (control.outline ? Theme.text : "#ffffff"))
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        implicitHeight: control.implicitHeight
        implicitWidth: control.implicitWidth
        radius: Theme.radiusSm
        color: {
            if (!control.enabled) return "#18181b";
            if (control.customColor != "transparent") {
                return control.down ? Qt.darker(control.customColor, 1.2) :
                       control.hovered ? Qt.lighter(control.customColor, 1.2) : control.customColor;
            }
            if (control.danger) {
                return control.down ? Theme.dangerHover :
                       control.hovered ? Qt.rgba(239/255, 68/255, 68/255, 0.2) :
                       (control.outline ? "transparent" : Theme.danger);
            }
            if (control.outline) {
                return control.down ? Theme.cardHover :
                       control.hovered ? Qt.rgba(255, 255, 255, 0.08) : "transparent";
            }
            return control.down ? Theme.accentHover :
                   control.hovered ? Qt.lighter(Theme.accent, 1.1) : Theme.accent;
        }
        border.color: {
            if (!control.enabled) return Theme.borderMuted;
            if (control.danger && control.outline) return Theme.danger;
            if (control.outline) return control.hovered ? Theme.textMuted : Theme.border;
            return "transparent";
        }
        border.width: 1
    }
}
