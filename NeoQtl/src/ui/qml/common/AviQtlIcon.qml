import "Icons.js" as Icons
import QtQuick

Text {
    id: root

    property string iconName: ""
    property alias size: root.font.pixelSize

    font.family: "remixicon"
    font.pixelSize: 24
    color: "white"
    text: Icons.RI[iconName] || ""
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
}
