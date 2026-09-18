import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common

Common.AviQtlWindow {
    id: root

    title: qsTr("AviQtlについて")
    width: 420
    height: 260
    minimumWidth: 420
    maximumWidth: 420
    minimumHeight: 260
    maximumHeight: 260

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        RowLayout {
            spacing: 20

            Image {
                source: "qrc:/assets/icon.svg"
                sourceSize.width: 64
                sourceSize.height: 64
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
                fillMode: Image.PreserveAspectFit
                smooth: true
                antialiasing: true
            }

            ColumnLayout {
                spacing: 4
                Layout.fillWidth: true

                Label {
                    text: "AviQtl " + (typeof AviQtlVersion !== "undefined" ? AviQtlVersion : "")
                    font.pixelSize: 28
                    font.bold: true
                    color: palette.text
                }

                Label {
                    text: typeof AviQtlVersionCodename !== "undefined" ? AviQtlVersionCodename : "Rolling Release"
                    font.pixelSize: 14
                    color: palette.highlight
                    font.bold: true
                }

            }

        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: palette.mid
            opacity: 0.5
        }

        Label {
            Layout.fillWidth: true
            text: qsTr("AviQtl は AviUtl ではありません。\n\nこのソフトウェアは GNU Affero General Public License Version 3 に基づいて公開されています。")
            wrapMode: Text.WordWrap
            font.pixelSize: 13
            color: palette.text
            lineHeight: 1.4
        }

        Label {
            Layout.fillWidth: true
            text: "<a href='https://codeberg.org/taisho-guy/AviQtl'>https://codeberg.org/taisho-guy/AviQtl</a>"
            textFormat: Text.RichText
            font.pixelSize: 13
            onLinkActivated: function(link) {
                Qt.openUrlExternally(link);
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.NoButton
                cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor
            }

        }

        Item {
            Layout.fillHeight: true
        }

    }

}
