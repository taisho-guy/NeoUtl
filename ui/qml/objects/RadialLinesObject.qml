import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property real sizeW: evalNumber("radial_lines", "sizeW", 1920)
    property real sizeH: evalNumber("radial_lines", "sizeH", 1080)
    property int lineCount: Math.max(1, Math.round(evalNumber("radial_lines", "lineCount", 128)))
    property real minLength: evalNumber("radial_lines", "minLength", 760)
    property real maxLength: evalNumber("radial_lines", "maxLength", 1500)
    property real thickness: evalNumber("radial_lines", "thickness", 5)
    property real randomness: evalNumber("radial_lines", "randomness", 0.75)
    property real centerX: evalNumber("radial_lines", "centerX", 0)
    property real centerY: evalNumber("radial_lines", "centerY", 0)
    property real spinSpeed: evalNumber("radial_lines", "spinSpeed", 0)
    property int seed: Math.round(evalNumber("radial_lines", "seed", 1))
    property color lineColor: evalColor("radial_lines", "color", "#ffffff")
    property real opacity: evalNumber("radial_lines", "opacity", 1)

    sourceItem: sourceItem

    function rand(n) {
        var x = Math.sin((n + seed * 101.3) * 12.9898) * 43758.5453;
        return x - Math.floor(x);
    }

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
                ctx.fillStyle = "#000000";
                ctx.fillRect(0, 0, width, height);
                ctx.save();
                ctx.translate(width / 2 + root.centerX, height / 2 + root.centerY);
                ctx.rotate(root.relFrame * root.spinSpeed * Math.PI / 180);
                ctx.lineCap = "round";
                ctx.strokeStyle = root.lineColor;

                var segments = [];
                for (var i = 0; i < root.lineCount; i++) {
                    var base = i / root.lineCount;
                    var jitter = (root.rand(i) - 0.5) * root.randomness * 0.055;
                    var a = (base + jitter) * Math.PI * 2;
                    var len = root.minLength + (root.maxLength - root.minLength) * root.rand(i + 900);
                    var start = 10 + root.rand(i + 1800) * root.maxLength * 0.08 * root.randomness;
                    var w = Math.max(0.1, root.thickness * (0.35 + root.rand(i + 2700) * 1.8));
                    var alpha = 0.16 + root.rand(i + 3600) * 0.58;
                    var cosA = Math.cos(a);
                    var sinA = Math.sin(a);
                    segments.push({
                        sx: cosA * start,
                        sy: sinA * start,
                        bx: cosA * (start + len * 0.08),
                        by: sinA * (start + len * 0.08),
                        ex: cosA * len,
                        ey: sinA * len,
                        w: w,
                        alpha: alpha
                    });
                }

                for (var layer = 0; layer < 3; layer++) {
                    for (var j = 0; j < segments.length; j++) {
                        var s = segments[j];
                        if (layer === 0) {
                            ctx.globalAlpha = s.alpha * 0.16;
                            ctx.lineWidth = s.w * 4.2;
                        } else if (layer === 1) {
                            ctx.globalAlpha = s.alpha * 0.34;
                            ctx.lineWidth = s.w * 1.8;
                        } else {
                            ctx.globalAlpha = s.alpha;
                            ctx.lineWidth = Math.max(0.6, s.w * 0.42);
                        }
                        ctx.beginPath();
                        if (layer === 2)
                            ctx.moveTo(s.bx, s.by);
                        else
                            ctx.moveTo(s.sx, s.sy);
                        ctx.lineTo(s.ex, s.ey);
                        ctx.stroke();
                    }
                }

                var centerGrad = ctx.createRadialGradient(0, 0, 0, 0, 0, Math.max(24, root.thickness * 9));
                centerGrad.addColorStop(0, "rgba(0,0,0,1)");
                centerGrad.addColorStop(1, "rgba(0,0,0,0)");
                ctx.globalAlpha = 0.95;
                ctx.fillStyle = centerGrad;
                ctx.beginPath();
                ctx.arc(0, 0, Math.max(24, root.thickness * 9), 0, Math.PI * 2);
                ctx.fill();
                ctx.globalAlpha = 1;
                ctx.restore();
            }

            Connections {
                target: root
                function onRevisionChanged() { canvas.requestPaint(); }
                function onRelFrameChanged() {
                    if (root.spinSpeed !== 0)
                        canvas.requestPaint();
                }
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
