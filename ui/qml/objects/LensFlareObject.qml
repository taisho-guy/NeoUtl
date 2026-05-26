import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property string objectId: "lens_flare_object"
    property real sizeW: evalNumber(objectId, "sizeW", 1920)
    property real sizeH: evalNumber(objectId, "sizeH", 1080)
    property real centerX: evalNumber(objectId, "centerX", 0)
    property real centerY: evalNumber(objectId, "centerY", 0)
    property real radius: evalNumber(objectId, "radius", 180)
    property real strength: evalNumber(objectId, "strength", 1)
    property int ghosts: Math.max(0, Math.round(evalNumber(objectId, "ghosts", 4)))
    property color flareColor: evalColor(objectId, "color", "#fff2aa")
    property real opacity: evalNumber(objectId, "opacity", 1)

    function rgbaString(c, a) {
        return "rgba(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + "," + Math.max(0, Math.min(1, a)) + ")";
    }

    sourceItem: sourceItem

    Item {
        id: sourceItem

        visible: false
        width: Math.max(1, root.sizeW)
        height: Math.max(1, root.sizeH)

        Canvas {
            id: canvas

            anchors.fill: parent
            antialiasing: true
            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                var cx = width / 2 + root.centerX;
                var cy = height / 2 + root.centerY;
                ctx.save();
                ctx.globalCompositeOperation = "lighter";
                var grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, root.radius);
                grad.addColorStop(0, root.rgbaString(root.flareColor, Math.min(1, root.strength)));
                grad.addColorStop(0.35, root.rgbaString(root.flareColor, Math.min(0.35, root.strength * 0.35)));
                grad.addColorStop(1, "rgba(255,255,255,0)");
                ctx.fillStyle = grad;
                ctx.fillRect(cx - root.radius, cy - root.radius, root.radius * 2, root.radius * 2);
                ctx.strokeStyle = root.rgbaString(root.flareColor, Math.min(0.75, root.strength * 0.55));
                ctx.lineWidth = Math.max(1, root.radius * 0.012);
                ctx.beginPath();
                ctx.moveTo(cx - root.radius * 1.3, cy);
                ctx.lineTo(cx + root.radius * 1.3, cy);
                ctx.moveTo(cx, cy - root.radius * 1.3);
                ctx.lineTo(cx, cy + root.radius * 1.3);
                ctx.stroke();
                for (var i = 0; i < root.ghosts; i++) {
                    var k = (i + 1) / (root.ghosts + 1);
                    var gx = width / 2 + (width / 2 - cx) * (k * 1.4 - 0.25);
                    var gy = height / 2 + (height / 2 - cy) * (k * 1.4 - 0.25);
                    var gr = root.radius * (0.08 + 0.08 * (i % 3));
                    var gg = ctx.createRadialGradient(gx, gy, 0, gx, gy, gr);
                    gg.addColorStop(0, root.rgbaString(root.flareColor, Math.min(0.45, root.strength * 0.35)));
                    gg.addColorStop(1, "rgba(255,255,255,0)");
                    ctx.fillStyle = gg;
                    ctx.fillRect(gx - gr, gy - gr, gr * 2, gr * 2);
                }
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
