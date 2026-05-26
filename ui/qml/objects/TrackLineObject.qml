import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property real sizeW: evalNumber("track_line", "sizeW", 1920)
    property real sizeH: evalNumber("track_line", "sizeH", 1080)
    property real startX: evalNumber("track_line", "startX", -240)
    property real startY: evalNumber("track_line", "startY", 120)
    property real endX: evalNumber("track_line", "endX", 240)
    property real endY: evalNumber("track_line", "endY", -120)
    property real lineWidth: evalNumber("track_line", "lineWidth", 8)
    property real dashLength: evalNumber("track_line", "dashLength", 0)
    property real dashSpace: evalNumber("track_line", "dashSpace", 12)
    property bool arrow: evalBool("track_line", "arrow", true)
    property color lineColor: evalColor("track_line", "color", "#ffffff")
    property real opacity: evalNumber("track_line", "opacity", 1)

    readonly property real padding: Math.max(24, lineWidth * 4)

    sourceItem: sourceItem

    Item {
        id: sourceItem

        visible: false
        width: Math.max(1, root.sizeW + root.padding * 2)
        height: Math.max(1, root.sizeH + root.padding * 2)

        Canvas {
            id: canvas

            anchors.fill: parent
            antialiasing: true
            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                var ox = width / 2;
                var oy = height / 2;
                var x1 = ox + root.startX;
                var y1 = oy + root.startY;
                var x2 = ox + root.endX;
                var y2 = oy + root.endY;
                ctx.save();
                ctx.strokeStyle = root.lineColor;
                ctx.fillStyle = root.lineColor;
                ctx.lineWidth = Math.max(0.1, root.lineWidth);
                ctx.lineCap = "round";
                ctx.lineJoin = "round";
                if (root.dashLength > 0)
                    ctx.setLineDash([root.dashLength, Math.max(0, root.dashSpace)]);
                ctx.beginPath();
                ctx.moveTo(x1, y1);
                ctx.lineTo(x2, y2);
                ctx.stroke();
                if (root.arrow) {
                    ctx.setLineDash([]);
                    var a = Math.atan2(y2 - y1, x2 - x1);
                    var len = Math.max(14, root.lineWidth * 4);
                    ctx.beginPath();
                    ctx.moveTo(x2, y2);
                    ctx.lineTo(x2 - Math.cos(a - Math.PI / 7) * len, y2 - Math.sin(a - Math.PI / 7) * len);
                    ctx.lineTo(x2 - Math.cos(a + Math.PI / 7) * len, y2 - Math.sin(a + Math.PI / 7) * len);
                    ctx.closePath();
                    ctx.fill();
                }
                ctx.restore();
            }

            Connections {
                target: root
                function onRevisionChanged() { canvas.requestPaint(); }
            }
        }
    }

    Model {
        source: "#Rectangle"
        visible: root.outputModelVisible
        scale: Qt.vector3d((root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.width : sourceItem.width) / 100, (root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.height : sourceItem.height) / 100, 1)
        opacity: root.opacity

        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            blendMode: root.blendMode
            cullMode: root.cullMode

            diffuseMap: Texture {
                sourceItem: root.displayOutput
            }
        }
    }
}
