import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../Components"

Window {
    id: dialog

    function open() { visible = true }
    function close() { visible = false }
    width: 580
    height: 460
    title: "NeoUtl - イージング編集"
    color: Theme.bg
    modality: Qt.WindowModal

    property string targetLabel: "Transform: X"
    property int curveType: 0
    readonly property var curveNames: ["直線 (Linear)", "イーズイン (EaseIn)", "イーズアウト (EaseOut)", "イーズインアウト (EaseInOut)", "バウンス (Bounce)"]

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            Text {
                text: dialog.targetLabel
                color: Theme.accent
                font.family: Theme.fontFamily
                font.pixelSize: 15
                font.bold: true
            }
            Item { Layout.fillWidth: true }
            ComboBox {
                model: dialog.curveNames
                onCurrentIndexChanged: {
                    dialog.curveType = currentIndex;
                    curveCanvas.requestPaint();
                }
            }
        }

                Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: Theme.bgDark
            border.color: Theme.border
            radius: Theme.radius

            Canvas {
                id: curveCanvas
                anchors.fill: parent
                anchors.margins: 20

                onPaint: {
                    var ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);

                                        ctx.strokeStyle = Theme.borderMuted;
                    ctx.beginPath();
                    ctx.moveTo(0, height);
                    ctx.lineTo(width, height);
                    ctx.moveTo(0, 0);
                    ctx.lineTo(0, height);
                    ctx.stroke();

                                        ctx.strokeStyle = Theme.accent;
                    ctx.lineWidth = 2;
                    ctx.beginPath();
                    ctx.moveTo(0, height);

                    for (var x = 0; x <= width; x += 2) {
                        var t = x / width;
                        var val = 0;
                        if (dialog.curveType === 0) val = t;
                        else if (dialog.curveType === 1) val = t * t;
                        else if (dialog.curveType === 2) val = 1 - (1 - t) * (1 - t);
                        else if (dialog.curveType === 3) val = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
                        else val = Math.abs(Math.sin(t * Math.PI * 2.5) * (1 - t));

                        var y = height - (val * height);
                        ctx.lineTo(x, y);
                    }
                    ctx.stroke();
                }
            }
        }

                RowLayout {
            Layout.fillWidth: true
            Item { Layout.fillWidth: true }
            NeoButton {
                text: "閉じる"
                onClicked: dialog.close()
            }
        }
    }
}
