import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property string objectId: "polygon_shape"
    property real sizeW: evalNumber(objectId, "sizeW", 1920)
    property real sizeH: evalNumber(objectId, "sizeH", 1080)
    property int sides: Math.max(3, Math.round(evalNumber(objectId, "sides", 6)))
    property real angle: evalNumber(objectId, "angle", 180)
    property real rotation: evalNumber(objectId, "rotation", 0)
    property color fillColor: evalColor(objectId, "color", "#66aa99")
    property color strokeColor: evalColor(objectId, "strokeColor", "#ffffff")
    property real strokeWidth: evalNumber(objectId, "strokeWidth", 0)
    property real opacity: evalNumber(objectId, "opacity", 1)
    readonly property real padding: strokeWidth / 2 + 4

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
                var cx = width / 2;
                var cy = height / 2;
                var rx = root.sizeW / 2;
                var ry = root.sizeH / 2;
                var rot = (root.rotation - 90) * Math.PI / 180;
                ctx.save();
                ctx.fillStyle = root.fillColor;
                ctx.strokeStyle = root.strokeColor;
                ctx.lineWidth = root.strokeWidth;
                ctx.lineJoin = "round";
                ctx.beginPath();
                if (root.objectId === "pie_shape") {
                    var sweep = Math.max(1, Math.min(360, root.angle));
                    var steps = Math.max(12, Math.ceil(sweep / 4));
                    ctx.moveTo(cx, cy);
                    for (var i = 0; i <= steps; i++) {
                        var a = rot + (sweep * i / steps) * Math.PI / 180;
                        ctx.lineTo(cx + Math.cos(a) * rx, cy + Math.sin(a) * ry);
                    }
                    ctx.closePath();
                } else {
                    for (var j = 0; j < root.sides; j++) {
                        var pa = rot + j * Math.PI * 2 / root.sides;
                        var x = cx + Math.cos(pa) * rx;
                        var y = cy + Math.sin(pa) * ry;
                        j === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
                    }
                    ctx.closePath();
                }
                ctx.fill();
                if (root.strokeWidth > 0)
                    ctx.stroke();

                ctx.restore();
            }

            Connections {
                function onRevisionChanged() {
                    canvas.requestPaint();
                }

                target: root
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
