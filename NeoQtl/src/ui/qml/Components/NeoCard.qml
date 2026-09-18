import QtQuick
import ".."

Rectangle {
    id: card

    property string heading: ""
    default
    property alias content: innerColumn.data

    color: Theme.card
    border.color: Theme.border
    border.width: 1
    radius: Theme.radius

    implicitHeight: mainLayout.implicitHeight + 24
    implicitWidth: 300

    Column {
        id: mainLayout
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Text {
            visible: card.heading.length > 0
            text: card.heading
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 14
            font.bold: true
        }

        Rectangle {
            visible: card.heading.length > 0
            width: parent.width
            height: 1
            color: Theme.border
        }

        Column {
            id: innerColumn
            width: parent.width
            spacing: 8
        }
    }
}
