import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property string objectId: "star"
    property real sizeW: evalNumber(objectId, "sizeW", 1920)
    property real sizeH: evalNumber(objectId, "sizeH", 1080)
    property int count: Math.max(1, Math.round(evalNumber(objectId, "count", 120)))
    property real speed: evalNumber(objectId, "speed", 1)
    property real particleSize: evalNumber(objectId, "particleSize", 4)
    property real spread: evalNumber(objectId, "spread", 1)
    property int seed: Math.round(evalNumber(objectId, "seed", 1))
    property color particleColor: evalColor(objectId, "color", "#ffffff")
    property real opacity: evalNumber(objectId, "opacity", 1)

    function rand(n) {
        var x = Math.sin((n + seed * 37.17) * 12.9898) * 43758.5;
        return x - Math.floor(x);
    }

    function drawStar(ctx, x, y, r, alpha) {
        ctx.globalAlpha = alpha;
        ctx.fillStyle = particleColor;
        ctx.beginPath();
        ctx.moveTo(x, y - r);
        ctx.lineTo(x + r * 0.26, y - r * 0.26);
        ctx.lineTo(x + r, y);
        ctx.lineTo(x + r * 0.26, y + r * 0.26);
        ctx.lineTo(x, y + r);
        ctx.lineTo(x - r * 0.26, y + r * 0.26);
        ctx.lineTo(x - r, y);
        ctx.lineTo(x - r * 0.26, y - r * 0.26);
        ctx.closePath();
        ctx.fill();
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
                ctx.save();
                ctx.fillStyle = root.particleColor;
                ctx.strokeStyle = root.particleColor;
                ctx.lineCap = "round";
                var t = root.relFrame * root.speed;
                for (var i = 0; i < root.count; i++) {
                    var rx = root.rand(i * 5 + 1);
                    var ry = root.rand(i * 5 + 2);
                    var rr = root.rand(i * 5 + 3);
                    var phase = root.rand(i * 5 + 4);
                    var x = rx * width;
                    var y = ry * height;
                    var s = root.particleSize * (0.45 + rr * 1.2);
                    var alpha = 0.35 + root.rand(i * 5 + 5) * 0.65;
                    if (root.objectId === "rain") {
                        x = (x + t * (10 + root.spread * 25) * (0.35 + rr)) % (width + 80) - 40;
                        y = (y + t * (28 + root.spread * 80) * (0.4 + phase)) % (height + 80) - 40;
                        ctx.globalAlpha = alpha;
                        ctx.lineWidth = Math.max(1, s * 0.32);
                        ctx.beginPath();
                        ctx.moveTo(x, y);
                        ctx.lineTo(x - s * 1.2, y + s * 5);
                        ctx.stroke();
                    } else if (root.objectId === "snow") {
                        x = (x + Math.sin((t * 0.03) + phase * Math.PI * 2) * root.spread * 36) % width;
                        y = (y + t * (3 + root.spread * 9) * (0.35 + rr)) % (height + 40) - 20;
                        ctx.globalAlpha = alpha;
                        ctx.beginPath();
                        ctx.arc(x, y, s, 0, Math.PI * 2);
                        ctx.fill();
                    } else {
                        var twinkle = 0.45 + Math.sin((t * 0.08) + phase * Math.PI * 2) * 0.35;
                        drawStar(ctx, x, y, s * (0.65 + twinkle * 0.35), Math.max(0.1, alpha * twinkle));
                    }
                }
                ctx.restore();
            }

            Connections {
                function onRevisionChanged() {
                    canvas.requestPaint();
                }

                function onRelFrameChanged() {
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
