import QtQuick
import QtQuick.Shapes
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property real sizeW: evalNumber("rect", "sizeW", 200)
    property real sizeH: evalNumber("rect", "sizeH", 200)
    property int sides: Math.max(3, Math.round(evalNumber("rect", "sides", 4)))
    property real cornerRadius: evalNumber("rect", "cornerRadius", 0)
    property real innerRadius: evalNumber("rect", "innerRadius", 50) // % (0-100)
    property real shapeRotDeg: evalNumber("rect", "rotation", 0) // 初期回転(°)
    property color fillColor: evalColor("rect", "color", "#66aa99")
    property bool useGradient: evalBool("rect", "useGradient", false)
    property color gradientColor2: evalColor("rect", "gradientColor2", "#ffffff")
    property int gradientType: Math.round(evalNumber("rect", "gradientType", 0)) // 0:Linear, 1:Radial
    property color strokeColor: evalColor("rect", "strokeColor", "#ffffff")
    property real strokeWidth: evalNumber("rect", "strokeWidth", 0)
    property real dashLength: evalNumber("rect", "dashLength", 0)
    property real dashSpace: evalNumber("rect", "dashSpace", 0)
    property real opacity: evalNumber("rect", "opacity", 1)
    property string shapeType: evalString("rect", "shapeType", "polygon")
    // 縁取りが見切れないようにパディングを確保
    readonly property real padding: strokeWidth / 2 + 2

    sourceItem: sourceItem

    Item {
        id: sourceItem

        visible: false
        width: root.sizeW + padding * 2
        height: root.sizeH + padding * 2

        // 描画ホスト
        Item {
            id: shapeHost

            anchors.centerIn: parent
            width: root.sizeW
            height: root.sizeH

            Canvas {
                id: shapeCanvas

                // 親のサイズ(sizeW/H)ではなく、padding込みのsourceItemサイズに合わせる
                width: sourceItem.width
                height: sourceItem.height
                anchors.centerIn: parent
                antialiasing: true
                // パラメーター変化 → 再描画
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    ctx.save();
                    // 中心座標 (パディング込みのキャンバス中心)
                    var cx = width / 2;
                    var cy = height / 2;
                    var n = root.sides;
                    // 回転(ラジアン)
                    var rot = root.shapeRotDeg * Math.PI / 180;
                    var cosR = Math.cos(rot);
                    var sinR = Math.sin(rot);
                    var type = root.shapeType;
                    // 塗りつぶしスタイルの設定
                    if (root.useGradient) {
                        var grad;
                        if (root.gradientType === 1)
                            grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(root.sizeW, root.sizeH) / 2);
                        else
                            grad = ctx.createLinearGradient(cx, cy - root.sizeH / 2, cx, cy + root.sizeH / 2);
                        grad.addColorStop(0, root.fillColor);
                        grad.addColorStop(1, root.gradientColor2);
                        ctx.fillStyle = grad;
                    } else {
                        ctx.fillStyle = root.fillColor;
                    }
                    ctx.strokeStyle = root.strokeColor;
                    ctx.lineWidth = root.strokeWidth;
                    ctx.lineJoin = "round";
                    // 破線設定
                    if (root.dashLength > 0 || root.dashSpace > 0)
                        ctx.setLineDash([Math.max(1, root.dashLength), Math.max(0, root.dashSpace)]);
                    else
                        ctx.setLineDash([]);
                    if (type === "pie" || type === "arc" || type === "donut") {
                        // 円・弧・ドーナツ系は sizeW / sizeH をそのまま楕円の半径として扱う
                        var arcDeg = Math.min(360, Math.max(1, n));
                        // -90度(12時方向)基準で回転させる
                        var startAng = (root.shapeRotDeg - 90) * Math.PI / 180;
                        var rx = root.sizeW / 2;
                        var ry = root.sizeH / 2;
                        var innerRx = rx * (root.innerRadius / 100);
                        var innerRy = ry * (root.innerRadius / 100);
                        var steps = Math.max(32, Math.ceil(arcDeg / 3));
                        ctx.beginPath();
                        if (type === "donut") {
                            for (var i = 0; i <= steps; i++) {
                                var a = startAng + (arcDeg * i / steps) * Math.PI / 180;
                                var px = cx + Math.cos(a) * rx;
                                var py = cy + Math.sin(a) * ry;
                                i === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
                            }
                            for (var j = steps; j >= 0; j--) {
                                var a = startAng + (arcDeg * j / steps) * Math.PI / 180;
                                var px = cx + Math.cos(a) * Math.max(1, innerRx);
                                var py = cy + Math.sin(a) * Math.max(1, innerRy);
                                ctx.lineTo(px, py);
                            }
                            ctx.closePath();
                        } else if (type === "arc") {
                            for (var i = 0; i <= steps; i++) {
                                var a = startAng + (arcDeg * i / steps) * Math.PI / 180;
                                var px = cx + Math.cos(a) * rx;
                                var py = cy + Math.sin(a) * ry;
                                i === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
                            }
                            if (root.strokeWidth > 0)
                                ctx.stroke();

                            ctx.restore();
                            return ;
                        } else {
                            // pie
                            ctx.moveTo(cx, cy);
                            for (var i = 0; i <= steps; i++) {
                                var a = startAng + (arcDeg * i / steps) * Math.PI / 180;
                                var px = cx + Math.cos(a) * rx;
                                var py = cy + Math.sin(a) * ry;
                                ctx.lineTo(px, py);
                            }
                            ctx.closePath();
                        }
                    } else {
                        var rawVerts = [];
                        var baseRot = -Math.PI / 2 - (Math.PI / n) * (n % 2 === 0 ? 1 : 0);
                        if (type === "star") {
                            var totalPts = n * 2;
                            for (var si = 0; si < totalPts; si++) {
                                var ang = baseRot + si * Math.PI / n;
                                var r = (si % 2 === 0) ? 1 : (root.innerRadius / 100);
                                var rx_ = Math.cos(ang) * r;
                                var ry_ = Math.sin(ang) * r;
                                rawVerts.push({
                                    "x": rx_ * cosR - ry_ * sinR,
                                    "y": rx_ * sinR + ry_ * cosR
                                });
                            }
                        } else {
                            // polygon
                            for (var vi = 0; vi < n; vi++) {
                                var va = baseRot + vi * 2 * Math.PI / n;
                                var vx_ = Math.cos(va);
                                var vy_ = Math.sin(va);
                                rawVerts.push({
                                    "x": vx_ * cosR - vy_ * sinR,
                                    "y": vx_ * sinR + vy_ * cosR
                                });
                            }
                        }
                        // バウンディングボックスの算出
                        var minX = rawVerts[0].x, maxX = rawVerts[0].x;
                        var minY = rawVerts[0].y, maxY = rawVerts[0].y;
                        for (var k = 1; k < rawVerts.length; k++) {
                            minX = Math.min(minX, rawVerts[k].x);
                            maxX = Math.max(maxX, rawVerts[k].x);
                            minY = Math.min(minY, rawVerts[k].y);
                            maxY = Math.max(maxY, rawVerts[k].y);
                        }
                        var spanX = maxX - minX || 1;
                        var spanY = maxY - minY || 1;
                        var verts = [];
                        for (var k = 0; k < rawVerts.length; k++) {
                            // -0.5 ~ 0.5 の範囲に正規化し、sizeW / sizeH を掛ける
                            var nx = (rawVerts[k].x - minX) / spanX - 0.5;
                            var ny = (rawVerts[k].y - minY) / spanY - 0.5;
                            verts.push([cx + nx * root.sizeW, cy + ny * root.sizeH]);
                        }
                        var cr = type === "star" ? 0 : root.cornerRadius;
                        ctx.beginPath();
                        if (cr < 0.5 || type === "star") {
                            for (var ki = 0; ki < verts.length; ki++) {
                                ki === 0 ? ctx.moveTo(verts[ki][0], verts[ki][1]) : ctx.lineTo(verts[ki][0], verts[ki][1]);
                            }
                            ctx.closePath();
                        } else {
                            // 角丸付き多角形
                            var vertsCount = verts.length;
                            for (var ki = 0; ki < vertsCount; ki++) {
                                var prev = verts[(ki - 1 + vertsCount) % vertsCount];
                                var curr = verts[ki];
                                var next = verts[(ki + 1) % vertsCount];
                                var dx1 = curr[0] - prev[0], dy1 = curr[1] - prev[1];
                                var len1 = Math.sqrt(dx1 * dx1 + dy1 * dy1);
                                var dx2 = next[0] - curr[0], dy2 = next[1] - curr[1];
                                var len2 = Math.sqrt(dx2 * dx2 + dy2 * dy2);
                                // 丸め半径が辺の半分を超えないように制限
                                var r2 = Math.min(cr, len1 / 2, len2 / 2);
                                var ax = curr[0] - (dx1 / len1) * r2;
                                var ay = curr[1] - (dy1 / len1) * r2;
                                ki === 0 ? ctx.moveTo(ax, ay) : ctx.lineTo(ax, ay);
                                ctx.arcTo(curr[0], curr[1], curr[0] + (dx2 / len2) * r2, curr[1] + (dy2 / len2) * r2, r2);
                            }
                            ctx.closePath();
                        }
                    }
                    ctx.fill();
                    if (root.strokeWidth > 0)
                        ctx.stroke();

                    ctx.restore();
                }

                // 全パラメーター監視して再描画
                Connections {
                    function onSizeWChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onSizeHChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onSidesChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onCornerRadiusChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onInnerRadiusChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onShapeRotDegChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onFillColorChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onUseGradientChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onGradientColor2Changed() {
                        shapeCanvas.requestPaint();
                    }

                    function onGradientTypeChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onStrokeColorChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onStrokeWidthChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onDashLengthChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onDashSpaceChanged() {
                        shapeCanvas.requestPaint();
                    }

                    function onShapeTypeChanged() {
                        shapeCanvas.requestPaint();
                    }

                    target: root
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
