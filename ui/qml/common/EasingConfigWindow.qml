import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: root

    property int clipId: -1
    property int effectIndex: -1
    property var effectModel: null
    property string paramName: ""
    property int keyframeFrame: 0
    property string selectedType: "none"
    property int stepFrames: 1
    property var bezierParams: [0.33, 0, 0.66, 1, 1, 1]
    property double elasticAmplitude: 1
    property double elasticPeriod: 0.3
    property real previewScale: 1 // 0.25 ～ 4.0
    property real previewOffsetX: 0 // 論理座標 (0-1空間) での平行移動
    property real previewOffsetY: 0
    property bool isInitializing: false

    function requestPreview() {
        if (previewCanvas)
            previewCanvas.requestPaint();

    }

    function evalEasing(t) {
        return evalEasingByType(t, root.selectedType);
    }

    function evalEasingByType(t, type) {
        function _bounceOut(x) {
            var n1 = 7.5625, d1 = 2.75;
            if (x < 1 / d1)
                return n1 * x * x;

            if (x < 2 / d1) {
                x -= 1.5 / d1;
                return n1 * x * x + 0.75;
            }
            if (x < 2.5 / d1) {
                x -= 2.25 / d1;
                return n1 * x * x + 0.9375;
            }
            x -= 2.625 / d1;
            return n1 * x * x + 0.984375;
        }

        var bz = root.bezierParams;
        if (type === "none")
            return t >= 1 ? 1 : 0;

        if (type === "linear")
            return t;

        if (type === "random") {
            var step = Math.max(1, root.stepFrames);
            var idx = Math.floor(t * 16 / step);
            var n = Math.abs(Math.sin((idx + 1) * 12.9898) * 43758.5);
            return n - Math.floor(n);
        }
        if (type === "alternate") {
            var altStep = Math.max(1, root.stepFrames);
            return Math.floor(t * 16 / altStep) % 2 === 0 ? 0 : 1;
        }
        if (type === "ease_in_sine")
            return 1 - Math.cos(t * Math.PI / 2);

        if (type === "ease_out_sine")
            return Math.sin(t * Math.PI / 2);

        if (type === "ease_in_out_sine")
            return -(Math.cos(Math.PI * t) - 1) / 2;

        if (type === "ease_out_in_sine")
            return t < 0.5 ? Math.sin(t * Math.PI) / 2 : (1 - Math.cos((t * 2 - 1) * Math.PI / 2)) / 2 + 0.5;

        if (type === "ease_in_quad")
            return t * t;

        if (type === "ease_out_quad")
            return 1 - (1 - t) * (1 - t);

        if (type === "ease_in_out_quad")
            return t < 0.5 ? 2 * t * t : 1 - ((-2 * t + 2) * (-2 * t + 2)) / 2;

        if (type === "ease_out_in_quad")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 2) / 2 : (2 * t - 1) ** 2 / 2 + 0.5;

        if (type === "ease_in_cubic")
            return t * t * t;

        if (type === "ease_out_cubic")
            return 1 - (1 - t) ** 3;

        if (type === "ease_in_out_cubic")
            return t < 0.5 ? 4 * t ** 3 : 1 - ((-2 * t + 2) ** 3) / 2;

        if (type === "ease_out_in_cubic")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 3) / 2 : (2 * t - 1) ** 3 / 2 + 0.5;

        if (type === "ease_in_quart")
            return t ** 4;

        if (type === "ease_out_quart")
            return 1 - (1 - t) ** 4;

        if (type === "ease_in_out_quart")
            return t < 0.5 ? 8 * t ** 4 : 1 - ((-2 * t + 2) ** 4) / 2;

        if (type === "ease_out_in_quart")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 4) / 2 : (2 * t - 1) ** 4 / 2 + 0.5;

        if (type === "ease_out_in_quart")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 4) / 2 : (2 * t - 1) ** 4 / 2 + 0.5;

        if (type === "ease_in_quint")
            return t ** 5;

        if (type === "ease_out_quint")
            return 1 - (1 - t) ** 5;

        if (type === "ease_in_out_quint")
            return t < 0.5 ? 16 * t ** 5 : 1 - ((-2 * t + 2) ** 5) / 2;

        if (type === "ease_out_in_quint")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 5) / 2 : (2 * t - 1) ** 5 / 2 + 0.5;

        if (type === "ease_out_in_quint")
            return t < 0.5 ? (1 - (1 - 2 * t) ** 5) / 2 : (2 * t - 1) ** 5 / 2 + 0.5;

        if (type === "ease_in_expo")
            return t === 0 ? 0 : Math.pow(2, 10 * t - 10);

        if (type === "ease_out_expo")
            return t === 1 ? 1 : 1 - Math.pow(2, -10 * t);

        if (type === "ease_in_out_expo") {
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            return t < 0.5 ? Math.pow(2, 20 * t - 10) / 2 : (2 - Math.pow(2, -20 * t + 10)) / 2;
        }
        if (type === "ease_out_in_expo") {
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            return t < 0.5 ? (1 - Math.pow(2, -20 * t)) / 2 : Math.pow(2, 20 * t - 20) / 2 + 0.5;
        }
        if (type === "ease_in_circ")
            return 1 - Math.sqrt(1 - t * t);

        if (type === "ease_out_circ")
            return Math.sqrt(1 - (t - 1) ** 2);

        if (type === "ease_in_out_circ")
            return t < 0.5 ? (1 - Math.sqrt(1 - 4 * t * t)) / 2 : (Math.sqrt(1 - (-2 * t + 2) ** 2) + 1) / 2;

        if (type === "ease_out_in_circ")
            return t < 0.5 ? Math.sqrt(1 - (2 * t - 1) ** 2) / 2 : (1 - Math.sqrt(1 - (2 * t - 1) ** 2)) / 2 + 0.5;

        if (type === "ease_in_back") {
            var c1 = 1.70158, c3 = c1 + 1;
            return c3 * t ** 3 - c1 * t ** 2;
        }
        if (type === "ease_out_back") {
            var c1b = 1.70158, c3b = c1b + 1;
            return 1 + c3b * (t - 1) ** 3 + c1b * (t - 1) ** 2;
        }
        if (type === "ease_in_out_back") {
            var c2 = 1.70158 * 1.525;
            return t < 0.5 ? ((2 * t) ** 2 * ((c2 + 1) * 2 * t - c2)) / 2 : ((2 * t - 2) ** 2 * ((c2 + 1) * (2 * t - 2) + c2) + 2) / 2;
        }
        if (type === "ease_out_in_back") {
            var c1 = 1.70158, c3 = c1 + 1;
            var eout = (u) => {
                return 1 + c3 * (u - 1) ** 3 + c1 * (u - 1) ** 2;
            };
            var ein = (u) => {
                return c3 * u ** 3 - c1 * u ** 2;
            };
            return t < 0.5 ? eout(2 * t) / 2 : ein(2 * t - 1) / 2 + 0.5;
        }
        if (type === "ease_in_elastic") {
            var c4 = 2 * Math.PI / 3;
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            return -Math.pow(2, 10 * t - 10) * Math.sin((10 * t - 10.75) * c4);
        }
        if (type === "ease_out_elastic") {
            var c4e = 2 * Math.PI / 3;
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            return Math.pow(2, -10 * t) * Math.sin((10 * t - 0.75) * c4e) + 1;
        }
        if (type === "ease_in_out_elastic") {
            var c5 = 2 * Math.PI / (root.elasticPeriod * 1.5);
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            return t < 0.5 ? -(root.elasticAmplitude * Math.pow(2, 20 * t - 10) * Math.sin((20 * t - 11.125) * c5)) / 2 : (root.elasticAmplitude * Math.pow(2, -20 * t + 10) * Math.sin((20 * t - 11.125) * c5)) / 2 + 1;
        }
        if (type === "ease_out_in_elastic") {
            var c4 = 2 * Math.PI / root.elasticPeriod;
            if (t === 0)
                return 0;

            if (t === 1)
                return 1;

            var eout = (u) => {
                return root.elasticAmplitude * Math.pow(2, -10 * u) * Math.sin((u - root.elasticPeriod / 4) * c4) + 1;
            };
            var ein = (u) => {
                return -root.elasticAmplitude * Math.pow(2, 10 * u - 10) * Math.sin((u - 1 - root.elasticPeriod / 4) * c4);
            };
            return t < 0.5 ? eout(2 * t) / 2 : ein(2 * t - 1) / 2 + 0.5;
        }
        if (type === "ease_out_bounce")
            return _bounceOut(t);

        if (type === "ease_in_bounce")
            return 1 - _bounceOut(1 - t);

        if (type === "ease_in_out_bounce")
            return t < 0.5 ? (1 - _bounceOut(1 - 2 * t)) / 2 : (1 + _bounceOut(2 * t - 1)) / 2;

        if (type === "ease_out_in_bounce")
            return t < 0.5 ? _bounceOut(2 * t) / 2 : (1 - _bounceOut(1 - 2 * (t - 0.5))) / 2 + 0.5;

        if (type === "custom") {
            var prevX = 0, prevY = 0;
            for (var i = 0; i < bz.length; i += 6) {
                var cp1x = bz[i], cp1y = bz[i + 1], cp2x = bz[i + 2], cp2y = bz[i + 3], endX = bz[i + 4], endY = bz[i + 5];
                if (t <= endX || i + 6 >= bz.length) {
                    var range = endX - prevX;
                    if (range < 0.0001)
                        return endY;

                    var nx = (t - prevX) / range, nc1x = (cp1x - prevX) / range, nc2x = (cp2x - prevX) / range, T = nx;
                    for (var k = 0; k < 8; k++) {
                        var u = 1 - T, cx = 3 * u * u * T * nc1x + 3 * u * T * T * nc2x + T * T * T, err = cx - nx;
                        if (Math.abs(err) < 0.0001)
                            break;

                        var dcx = 3 * u * u * nc1x + 6 * u * T * (nc2x - nc1x) + 3 * T * T * (1 - nc2x);
                        if (Math.abs(dcx) < 1e-06)
                            break;

                        T -= err / dcx;
                    }
                    T = Math.max(0, Math.min(1, T));
                    var u2 = 1 - T;
                    return u2 ** 3 * prevY + 3 * u2 ** 2 * T * cp1y + 3 * u2 * T ** 2 * cp2y + T ** 3 * endY;
                }
                prevX = endX;
                prevY = endY;
            }
        }
        return t;
    }

    function textColor(alpha) {
        return Qt.rgba(root.palette.text.r, root.palette.text.g, root.palette.text.b, alpha);
    }

    function panelColor(alpha) {
        return Qt.rgba(root.palette.base.r, root.palette.base.g, root.palette.base.b, alpha);
    }

    function itemFamily(type) {
        if (type.indexOf("_sine") !== -1)
            return qsTr("サイン");

        if (type.indexOf("_quad") !== -1)
            return qsTr("2次");

        if (type.indexOf("_cubic") !== -1)
            return qsTr("3次");

        if (type.indexOf("_quart") !== -1)
            return qsTr("4次");

        if (type.indexOf("_quint") !== -1)
            return qsTr("5次");

        if (type.indexOf("_expo") !== -1)
            return qsTr("指数");

        if (type.indexOf("_circ") !== -1)
            return qsTr("円");

        if (type.indexOf("_back") !== -1)
            return qsTr("戻る");

        if (type.indexOf("_elastic") !== -1)
            return qsTr("弾性");

        if (type.indexOf("_bounce") !== -1)
            return qsTr("跳ね返り");

        return "";
    }

    function itemDirection(type) {
        if (type.indexOf("_in_out_") !== -1)
            return qsTr("加減速");

        if (type.indexOf("_out_in_") !== -1)
            return qsTr("減加速");

        if (type.indexOf("_in_") !== -1)
            return qsTr("加速");

        if (type.indexOf("_out_") !== -1)
            return qsTr("減速");

        return "";
    }

    function itemLabel(type) {
        if (type === "none")
            return qsTr("瞬間移動");

        if (type === "linear")
            return qsTr("直線");

        if (type === "custom")
            return qsTr("カスタム");

        if (type === "random")
            return qsTr("ランダム移動");

        if (type === "alternate")
            return qsTr("反復移動");

        var family = itemFamily(type);
        var direction = itemDirection(type);
        if (family !== "" && direction !== "")
            return family + " " + direction;

        return type;
    }

    function openConfig(args) {
        isInitializing = true;
        clipId = args.clipId;
        effectIndex = args.effectIndex;
        effectModel = args.effectModel;
        paramName = args.paramName;
        keyframeFrame = args.keyframeFrame;
        show();
        raise();
        requestActivate();
        previewScale = 1;
        previewOffsetX = 0;
        previewOffsetY = 0;
        {
            const _clipDur = Workspace.currentTimeline ? Workspace.currentTimeline.clipDurationFrames : 100;
            const _fps = (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60;
            const _endFrame = _clipDur;
            const _track = effectModel.keyframeListForUi(paramName) || [];
            const _hasKfAtEnd = _track.some(function(kf) {
                return kf.frame === _endFrame;
            });
            if (!_hasKfAtEnd) {
                const _endVal = effectModel.evaluatedParam(paramName, _endFrame, _fps);
                // endFrame 自体は末尾なので interp は使われない（none のまま）
                Workspace.currentTimeline.setKeyframe(clipId, effectIndex, paramName, _endFrame, _endVal, {
                    "interp": "none"
                });
                Qt.callLater(function() {
                    if (!isInitializing)
                        updateKeyframe();

                });
            }
        };
        typeCombo.model = effectModel.availableEasings();
        const tracks = effectModel.keyframeTracks;
        const track = effectModel ? effectModel.keyframeListForUi(paramName) : undefined;
        if (!track)
            return ;

        for (let i = 0; i < track.length; i++) {
            if (track[i].frame !== keyframeFrame)
                continue;

            const kf = track[i];
            selectedType = kf.interp || "none";
            stepFrames = (kf.modeParams && kf.modeParams.stepFrames) ? kf.modeParams.stepFrames : 1;
            elasticAmplitude = (kf.modeParams && kf.modeParams.amplitude) ? kf.modeParams.amplitude : 1;
            elasticPeriod = (kf.modeParams && kf.modeParams.period) ? kf.modeParams.period : 0.3;
            if (selectedType === "bezier")
                selectedType = "custom";

            var idx = typeCombo.model.indexOf(selectedType);
            typeCombo.currentIndex = idx >= 0 ? idx : 0;
            if (selectedType === "custom") {
                if (kf.points && kf.points.length >= 6) {
                    var pts = [];
                    for (let j = 0; j < kf.points.length; j++) pts.push(kf.points[j])
                    bezierParams = pts;
                } else {
                    bezierParams = [kf.bzx1 !== undefined ? kf.bzx1 : 0.33, kf.bzy1 !== undefined ? kf.bzy1 : 0, kf.bzx2 !== undefined ? kf.bzx2 : 0.66, kf.bzy2 !== undefined ? kf.bzy2 : 1, 1, 1];
                }
            }
            break;
        }
        requestPreview();
        isInitializing = false;
    }

    function updateKeyframe() {
        if (!effectModel)
            return ;

        const kf = effectModel.keyframeListForUi(paramName).find((k) => {
            return k.frame === keyframeFrame;
        });
        if (!kf)
            return ;

        let options = {
            "interp": selectedType
        };
        if (selectedType === "custom") {
            var pts = [];
            for (let j = 0; j < bezierParams.length; j++) pts.push(bezierParams[j])
            options.points = pts;
        }
        if (selectedType === "random" || selectedType === "alternate")
            options.modeParams = {
            "stepFrames": Math.max(1, stepFrames)
        };

        if (selectedType.indexOf("elastic") !== -1)
            options.modeParams = {
            "amplitude": elasticAmplitude,
            "period": elasticPeriod
        };

        Workspace.currentTimeline.setKeyframe(clipId, effectIndex, paramName, keyframeFrame, kf.value, options);
    }

    title: qsTr("補間設定: %1").arg(paramName)
    width: 820
    height: 540
    onSelectedTypeChanged: {
        requestPreview();
        if (!isInitializing)
            updateKeyframe();

    }
    onBezierParamsChanged: {
        requestPreview();
        if (!isInitializing)
            updateKeyframe();

    }
    onStepFramesChanged: {
        requestPreview();
        if (!isInitializing)
            updateKeyframe();

    }
    onElasticAmplitudeChanged: {
        requestPreview();
        if (!isInitializing)
            updateKeyframe();

    }
    onElasticPeriodChanged: {
        requestPreview();
        if (!isInitializing)
            updateKeyframe();

    }
    onPreviewScaleChanged: requestPreview()
    onPreviewOffsetXChanged: requestPreview()
    onPreviewOffsetYChanged: requestPreview()

    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal

        Item {
            SplitView.fillWidth: true
            SplitView.minimumWidth: 360
            SplitView.preferredWidth: 520

            ColumnLayout {
                anchors.fill: parent
                spacing: 6

                // ズームコントロール
                RowLayout {
                    spacing: 6

                    Label {
                        text: qsTr("プレビュー")
                        font.bold: true
                        color: palette.text
                    }

                    Item {
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("ズーム:")
                        font.pixelSize: 11
                        color: root.textColor(0.7)
                    }

                    Button {
                        text: "−"
                        flat: true
                        implicitWidth: 24
                        implicitHeight: 24
                        onClicked: root.previewScale = Math.max(0.25, root.previewScale / 1.4)
                    }

                    Label {
                        text: Math.round(root.previewScale * 100) + "%"
                        font.pixelSize: 11
                        color: palette.text
                        Layout.preferredWidth: 36
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Button {
                        text: "+"
                        flat: true
                        implicitWidth: 24
                        implicitHeight: 24
                        onClicked: root.previewScale = Math.min(4, root.previewScale * 1.4)
                    }

                    Button {
                        text: "1:1"
                        flat: true
                        font.pixelSize: 10
                        implicitWidth: 30
                        implicitHeight: 24
                        onClicked: {
                            root.previewScale = 1;
                            root.previewOffsetX = 0;
                            root.previewOffsetY = 0;
                        }
                    }

                }

                // プレビューキャンバス本体
                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: palette.base
                    border.color: palette.mid
                    border.width: 1
                    radius: 4
                    clip: true

                    Canvas {
                        id: previewCanvas

                        function lxToPx(lx) {
                            var scale = root.previewScale;
                            var offX = root.previewOffsetX;
                            return (lx - offX) * scale * width + width * (1 - scale) / 2 + width * offX * scale;
                        }

                        function lyToPy(ly) {
                            var scale = root.previewScale;
                            var offY = root.previewOffsetY;
                            return height - ((ly - offY) * scale * height + height * (1 - scale) / 2 + height * offY * scale);
                        }

                        // ピクセル → 論理座標（ドラッグ用）
                        function pxToLx(px) {
                            var scale = root.previewScale;
                            return (px - width * (1 - scale) / 2) / (scale * width) + root.previewOffsetX * (1 - 1 / scale);
                        }

                        function pyToLy(py) {
                            var scale = root.previewScale;
                            return 1 - ((py - height * (1 - scale) / 2) / (scale * height) + root.previewOffsetY * (1 - 1 / scale));
                        }

                        anchors.fill: parent
                        onPaint: {
                            var ctx = getContext("2d");
                            var w = width, h = height;
                            ctx.clearRect(0, 0, w, h);
                            ctx.fillStyle = root.palette.window;
                            ctx.fillRect(0, 0, w, h);
                            // グリッド (論理0-1の格子)
                            ctx.strokeStyle = root.palette.mid;
                            ctx.lineWidth = 0.5;
                            ctx.globalAlpha = 0.3;
                            for (var gi = 0; gi <= 4; gi++) {
                                var gx = lxToPx(gi / 4), gy = lyToPy(gi / 4);
                                ctx.beginPath();
                                ctx.moveTo(gx, 0);
                                ctx.lineTo(gx, h);
                                ctx.stroke();
                                ctx.beginPath();
                                ctx.moveTo(0, gy);
                                ctx.lineTo(w, gy);
                                ctx.stroke();
                            }
                            ctx.globalAlpha = 1;
                            // 有効領域枠 (0-1 矩形)
                            ctx.strokeStyle = root.palette.mid;
                            ctx.lineWidth = 1;
                            var x0 = lxToPx(0), x1 = lxToPx(1), y0 = lyToPy(0), y1 = lyToPy(1);
                            ctx.strokeRect(x0, y1, x1 - x0, y0 - y1);
                            // 対角線 (linear 参照)
                            ctx.strokeStyle = root.palette.mid;
                            ctx.lineWidth = 1;
                            ctx.beginPath();
                            ctx.moveTo(x0, y0);
                            ctx.lineTo(x1, y1);
                            ctx.stroke();
                            // カーブ本体
                            ctx.strokeStyle = root.palette.highlight;
                            ctx.lineWidth = 2;
                            ctx.beginPath();
                            var steps = 128;
                            for (var s = 0; s <= steps; s++) {
                                var t = s / steps;
                                var y = root.evalEasing(t);
                                var px = lxToPx(t);
                                var py = lyToPy(y);
                                s === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
                            }
                            ctx.stroke();
                            // custom モード: タンジェントラインとハンドル描画
                            if (root.selectedType === "custom") {
                                var bz = root.bezierParams;
                                var prevAX = lxToPx(0), prevAY = lyToPy(0);
                                for (var seg = 0; seg < bz.length; seg += 6) {
                                    var c1px = lxToPx(bz[seg]), c1py = lyToPy(bz[seg + 1]);
                                    var c2px = lxToPx(bz[seg + 2]), c2py = lyToPy(bz[seg + 3]);
                                    var enpx = lxToPx(bz[seg + 4]), enpy = lyToPy(bz[seg + 5]);
                                    // タンジェントライン
                                    ctx.strokeStyle = root.palette.dark;
                                    ctx.lineWidth = 0.8;
                                    ctx.setLineDash([3, 3]);
                                    ctx.beginPath();
                                    ctx.moveTo(prevAX, prevAY);
                                    ctx.lineTo(c1px, c1py);
                                    ctx.stroke();
                                    ctx.beginPath();
                                    ctx.moveTo(enpx, enpy);
                                    ctx.lineTo(c2px, c2py);
                                    ctx.stroke();
                                    ctx.setLineDash([]);
                                    // cp1
                                    ctx.fillStyle = root.palette.text;
                                    ctx.beginPath();
                                    ctx.arc(c1px, c1py, 5, 0, 2 * Math.PI);
                                    ctx.fill();
                                    // cp2
                                    ctx.beginPath();
                                    ctx.arc(c2px, c2py, 5, 0, 2 * Math.PI);
                                    ctx.fill();
                                    // anchor (中間のみ)
                                    if (seg + 6 < bz.length) {
                                        ctx.fillStyle = root.palette.highlight;
                                        ctx.beginPath();
                                        ctx.arc(enpx, enpy, 6, 0, 2 * Math.PI);
                                        ctx.fill();
                                    }
                                    prevAX = enpx;
                                    prevAY = enpy;
                                }
                            }
                            // 座標軸ラベル
                            ctx.fillStyle = root.palette.mid;
                            ctx.font = "10px sans-serif";
                            ctx.fillText("0", lxToPx(-0.03), lyToPy(-0.04));
                            ctx.fillText("1", lxToPx(0.97), lyToPy(-0.04));
                            ctx.fillText("1", lxToPx(-0.06), lyToPy(1.02));
                        }
                    }

                    MouseArea {
                        id: panArea

                        property real lastX: 0
                        property real lastY: 0

                        anchors.fill: parent
                        acceptedButtons: Qt.RightButton
                        cursorShape: Qt.ClosedHandCursor
                        onPressed: (mouse) => {
                            lastX = mouse.x;
                            lastY = mouse.y;
                        }
                        onPositionChanged: (mouse) => {
                            if (!(mouse.buttons & Qt.RightButton))
                                return ;

                            var scale = root.previewScale;
                            root.previewOffsetX -= (mouse.x - lastX) / (scale * previewCanvas.width);
                            root.previewOffsetY += (mouse.y - lastY) / (scale * previewCanvas.height);
                            lastX = mouse.x;
                            lastY = mouse.y;
                        }
                        onWheel: (wheel) => {
                            var factor = wheel.angleDelta.y > 0 ? 1.2 : 1 / 1.2;
                            root.previewScale = Math.max(0.25, Math.min(4, root.previewScale * factor));
                            wheel.accepted = true;
                        }
                    }

                    MouseArea {
                        // cp1
                        // cp2
                        // new anchor

                        id: dragArea

                        property int dragIdx: -1

                        function findNearest(mx, my) {
                            var bz = root.bezierParams;
                            var best = -1, bestD = 12 * 12; // ヒット判定距離 (px)
                            for (var seg = 0; seg < bz.length; seg += 6) {
                                // cp1, cp2, and endAnchor
                                var pts = [[bz[seg], bz[seg + 1]], [bz[seg + 2], bz[seg + 3]], [bz[seg + 4], bz[seg + 5]]];
                                var indices = [seg, seg + 2, seg + 4];
                                for (var pi = 0; pi < 3; pi++) {
                                    var px = previewCanvas.lxToPx(pts[pi][0]);
                                    var py = previewCanvas.lyToPy(pts[pi][1]);
                                    var dx = mx - px, dy = my - py;
                                    if (dx * dx + dy * dy < bestD) {
                                        bestD = dx * dx + dy * dy;
                                        best = indices[pi];
                                    }
                                }
                            }
                            return best;
                        }

                        anchors.fill: parent
                        enabled: root.selectedType === "custom"
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        cursorShape: dragIdx >= 0 ? Qt.ClosedHandCursor : (findNearest(mouseX, mouseY) >= 0 ? Qt.PointingHandCursor : Qt.ArrowCursor)
                        onDoubleClicked: (mouse) => {
                            if (mouse.button !== Qt.LeftButton)
                                return ;

                            var lx = previewCanvas.pxToLx(mouse.x);
                            var ly = previewCanvas.pyToLy(mouse.y);
                            if (lx <= 0.01 || lx >= 0.99)
                                return ;

                            var p = root.bezierParams.slice();
                            // 挿入位置を特定 (endX の昇順を維持)
                            var insertIdx = 0;
                            for (var i = 0; i < p.length; i += 6) {
                                if (lx < p[i + 4]) {
                                    insertIdx = i;
                                    break;
                                }
                                insertIdx = i + 6;
                            }
                            // 新しいセグメントを構築
                            // 前のアンカーを取得
                            var prevX = (insertIdx === 0) ? 0 : p[insertIdx - 2];
                            var prevY = (insertIdx === 0) ? 0 : p[insertIdx - 1];
                            var newSeg = [prevX + (lx - prevX) * 0.33, prevY + (ly - prevY) * 0.33, prevX + (lx - prevX) * 0.66, prevY + (ly - prevY) * 0.66, lx, ly];
                            p.splice(insertIdx, 0, newSeg[0], newSeg[1], newSeg[2], newSeg[3], newSeg[4], newSeg[5]);
                            if (insertIdx + 6 < p.length) {
                                p[insertIdx + 6] = lx + (p[insertIdx + 10] - lx) * 0.33;
                                p[insertIdx + 7] = ly + (p[insertIdx + 11] - ly) * 0.33;
                            }
                            root.bezierParams = p;
                        }
                        onClicked: (mouse) => {
                            if (mouse.button === Qt.RightButton) {
                                var hit = findNearest(mouse.x, mouse.y);
                                if (hit >= 4 && (hit + 2) % 6 === 0 && hit < root.bezierParams.length - 2) {
                                    var p = root.bezierParams.slice();
                                    p.splice(hit - 4, 6);
                                    root.bezierParams = p;
                                }
                            }
                        }
                        onPressed: (mouse) => {
                            if (mouse.button === Qt.LeftButton)
                                dragIdx = findNearest(mouse.x, mouse.y);

                        }
                        onPositionChanged: (mouse) => {
                            if (dragIdx < 0)
                                return ;

                            var lx = previewCanvas.pxToLx(mouse.x);
                            var ly = previewCanvas.pyToLy(mouse.y);
                            var p = root.bezierParams.slice();
                            // 境界条件の処理
                            var isLastAnchor = (dragIdx === p.length - 2);
                            if (isLastAnchor) {
                                // 終端アンカーは (1,1) 固定
                                lx = 1;
                                ly = 1;
                            } else if ((dragIdx + 2) % 6 === 0) {
                                // 中間アンカーの場合、前後のアンカーを越えないようにする
                                var prevA = (dragIdx < 6) ? 0 : p[dragIdx - 6];
                                var nextA = p[dragIdx + 6]; // 次の cp1x ではなく次の anchorX は +6
                                // 正確には次のアンカーは dragIdx + 6 (cp1) + 4 = dragIdx + 10
                                if (dragIdx + 6 < p.length)
                                    nextA = p[dragIdx + 6];

                                lx = Math.max(0.001, Math.min(0.999, lx));
                            } else {
                                lx = Math.max(0, Math.min(1, lx));
                            }
                            p[dragIdx] = lx;
                            p[dragIdx + 1] = ly;
                            root.bezierParams = p;
                        }
                        onReleased: {
                            dragIdx = -1;
                        }
                    }

                    // ズームヒント
                    Label {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 4
                        text: qsTr("右ドラッグ:パン  ホイール:ズーム") + (root.selectedType === "custom" ? qsTr("  左ドラッグ:ハンドル") : "")
                        font.pixelSize: 9
                        color: root.textColor(0.65)
                    }

                }

                GroupBox {
                    title: qsTr("更新間隔:")
                    visible: root.selectedType === "random" || root.selectedType === "alternate"
                    Layout.fillWidth: true

                    RowLayout {
                        anchors.fill: parent
                        spacing: 6

                        SpinBox {
                            from: 1
                            to: 9999
                            value: root.stepFrames
                            onValueModified: root.stepFrames = value
                        }

                        Label {
                            text: qsTr("フレーム")
                            color: root.textColor(0.8)
                        }

                        Item {
                            Layout.fillWidth: true
                        }

                    }

                }

                GroupBox {
                    title: qsTr("詳細設定")
                    visible: root.selectedType.indexOf("elastic") !== -1
                    Layout.fillWidth: true

                    ColumnLayout {
                        anchors.fill: parent

                        RowLayout {
                            Label {
                                text: qsTr("振幅:")
                                Layout.preferredWidth: 40
                            }

                            Slider {
                                id: ampSlider

                                from: 0.1
                                to: 5
                                value: root.elasticAmplitude
                                onMoved: {
                                    root.elasticAmplitude = value;
                                    root.requestPreview();
                                }
                                Layout.fillWidth: true
                            }

                            Label {
                                text: root.elasticAmplitude.toFixed(2)
                                Layout.preferredWidth: 30
                            }

                        }

                        RowLayout {
                            Label {
                                text: qsTr("周期:")
                                Layout.preferredWidth: 40
                            }

                            Slider {
                                id: perSlider

                                from: 0.05
                                to: 1
                                value: root.elasticPeriod
                                onMoved: {
                                    root.elasticPeriod = value;
                                    root.requestPreview();
                                }
                                Layout.fillWidth: true
                            }

                            Label {
                                text: root.elasticPeriod.toFixed(2)
                                Layout.preferredWidth: 30
                            }

                        }

                    }

                }

                GroupBox {
                    title: qsTr("制御点")
                    enabled: root.selectedType === "custom"
                    opacity: root.selectedType === "custom" ? 1 : 0.35
                    Layout.fillWidth: true

                    GridLayout {
                        columns: 8
                        columnSpacing: 6
                        rowSpacing: 4

                        Repeater {
                            model: [{
                                "label": "CP1 X",
                                "idx": 0,
                                "clamp": true
                            }, {
                                "label": "CP1 Y",
                                "idx": 1,
                                "clamp": false
                            }, {
                                "label": "CP2 X",
                                "idx": 2,
                                "clamp": true
                            }, {
                                "label": "CP2 Y",
                                "idx": 3,
                                "clamp": false
                            }]

                            delegate: RowLayout {
                                spacing: 3

                                Label {
                                    text: modelData.label
                                    font.pixelSize: 11
                                    color: root.textColor(0.7)
                                }

                                TextField {
                                    id: cpField

                                    property int pIdx: modelData.idx
                                    property bool doClamp: modelData.clamp

                                    implicitWidth: 60
                                    font.pixelSize: 11
                                    text: root.bezierParams[pIdx].toFixed(3)
                                    selectByMouse: true
                                    onEditingFinished: {
                                        var p = root.bezierParams.slice();
                                        var v = parseFloat(text) || 0;
                                        p[pIdx] = doClamp ? Math.max(0, Math.min(1, v)) : v;
                                        root.bezierParams = p;
                                    }

                                    Binding on text {
                                        when: !cpField.activeFocus
                                        value: root.bezierParams[cpField.pIdx].toFixed(3)
                                    }

                                }

                            }

                        }

                    }

                }

            }
            // End of Item 1

        }

        Item {
            SplitView.minimumWidth: 240
            SplitView.preferredWidth: 280

            ColumnLayout {
                id: easingPanel

                // カテゴリ別グリッド
                property string filterText: ""
                // イージングカテゴリ定義
                property var categories: [{
                    "name": qsTr("基本"),
                    "items": ["none", "linear"]
                }, {
                    "name": qsTr("標準カーブ"),
                    "items": ["ease_in_sine", "ease_out_sine", "ease_in_out_sine", "ease_out_in_sine", "ease_in_quad", "ease_out_quad", "ease_in_out_quad", "ease_out_in_quad", "ease_in_cubic", "ease_out_cubic", "ease_in_out_cubic", "ease_out_in_cubic"]
                }, {
                    "name": qsTr("強いカーブ"),
                    "items": ["ease_in_quart", "ease_out_quart", "ease_in_out_quart", "ease_out_in_quart", "ease_in_quint", "ease_out_quint", "ease_in_out_quint", "ease_out_in_quint", "ease_in_expo", "ease_out_expo", "ease_in_out_expo", "ease_out_in_expo", "ease_in_circ", "ease_out_circ", "ease_in_out_circ", "ease_out_in_circ"]
                }, {
                    "name": qsTr("反動と弾性"),
                    "items": ["ease_in_back", "ease_out_back", "ease_in_out_back", "ease_out_in_back", "ease_in_elastic", "ease_out_elastic", "ease_in_out_elastic", "ease_out_in_elastic", "ease_in_bounce", "ease_out_bounce", "ease_in_out_bounce", "ease_out_in_bounce"]
                }, {
                    "name": qsTr("特殊"),
                    "items": ["random", "alternate", "custom"]
                }]

                anchors.fill: parent
                Layout.fillHeight: true
                Layout.fillWidth: true
                spacing: 6

                RowLayout {
                    Label {
                        text: qsTr("種類")
                        font.bold: true
                        color: palette.text
                    }

                    Item {
                        Layout.fillWidth: true
                    }
                    // テキスト検索フィルター

                    TextField {
                        id: searchField

                        placeholderText: qsTr("検索...")
                        implicitWidth: 110
                        font.pixelSize: 11
                        selectByMouse: true
                        // ComboBox は参照用として隠しで保持
                        onTextChanged: easingGrid.filterText = text.toLowerCase().replace(/_/g, "")
                    }
                    // 隠し ComboBox（既存 API 互換用）

                    ComboBox {
                        id: typeCombo

                        visible: false
                        model: ["linear"]
                        onActivated: (idx) => {
                            return root.selectedType = currentText;
                        }
                    }

                }

                ScrollView {
                    id: easingGrid

                    property string filterText: ""

                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

                    ColumnLayout {
                        width: easingGrid.availableWidth
                        spacing: 2

                        Repeater {
                            model: easingPanel.categories

                            delegate: ColumnLayout {
                                id: catCol

                                property var catData: modelData
                                property var visibleItems: {
                                    var ft = easingGrid.filterText;
                                    if (ft === "")
                                        return catData.items;

                                    return catData.items.filter((e) => {
                                        return e.toLowerCase().replace(/_/g, "").includes(ft);
                                    });
                                }

                                visible: visibleItems.length > 0
                                Layout.fillWidth: true
                                spacing: 1

                                Label {
                                    text: catCol.catData.name
                                    font.pixelSize: 10
                                    font.bold: true
                                    color: root.textColor(0.75)
                                    leftPadding: 2
                                    topPadding: 4
                                }

                                GridLayout {
                                    columns: 4
                                    columnSpacing: 4
                                    rowSpacing: 4
                                    Layout.fillWidth: true

                                    Repeater {
                                        model: catCol.visibleItems

                                        delegate: Button {
                                            id: easingButton

                                            property string easingName: modelData
                                            property bool isCurrent: root.selectedType === easingName

                                            // ミニプレビューキャンバス付きボタン
                                            Layout.fillWidth: true
                                            Layout.preferredHeight: 64
                                            flat: true
                                            padding: 0
                                            onClicked: {
                                                root.selectedType = easingName;
                                                var idx = typeCombo.model.indexOf(easingName);
                                                if (idx >= 0)
                                                    typeCombo.currentIndex = idx;

                                            }

                                            ColumnLayout {
                                                anchors.fill: parent
                                                anchors.margins: 4
                                                spacing: 1

                                                // ミニプレビュー
                                                Canvas {
                                                    id: miniCanvas

                                                    property string etype: easingButton.easingName

                                                    // ミニプレビュー専用軽量評価
                                                    function miniEval(t, type) {
                                                        return root.evalEasingByType(t, type);
                                                    }

                                                    Layout.fillWidth: true
                                                    Layout.fillHeight: true
                                                    Component.onCompleted: requestPaint()
                                                    onPaint: {
                                                        var ctx = getContext("2d");
                                                        var w = width, h = height;
                                                        ctx.clearRect(0, 0, w, h);
                                                        ctx.fillStyle = easingButton.isCurrent ? root.palette.highlight : root.palette.base;
                                                        ctx.fillRect(0, 0, w, h);
                                                        // カーブ
                                                        ctx.strokeStyle = easingButton.isCurrent ? root.palette.highlightedText : root.palette.highlight;
                                                        ctx.lineWidth = 1.5;
                                                        ctx.beginPath();
                                                        var steps = 48;
                                                        for (var s = 0; s <= steps; s++) {
                                                            var t = s / steps;
                                                            // evalEasing を root 経由で呼ぶために一時的に selectedType を使えないため
                                                            // ミニプレビュー専用のシンプルな評価
                                                            var y = miniEval(t, etype);
                                                            var px = t * w;
                                                            var py = h - Math.max(-0.3, Math.min(1.3, y)) * h;
                                                            s === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
                                                        }
                                                        ctx.stroke();
                                                    }

                                                    Connections {
                                                        function onSelectedTypeChanged() {
                                                            miniCanvas.requestPaint();
                                                        }

                                                        function onStepFramesChanged() {
                                                            miniCanvas.requestPaint();
                                                        }

                                                        target: root
                                                    }

                                                }

                                                Label {
                                                    text: root.itemLabel(easingButton.easingName)
                                                    font.pixelSize: 9
                                                    color: easingButton.isCurrent ? root.palette.highlightedText : root.textColor(0.82)
                                                    elide: Text.ElideRight
                                                    Layout.fillWidth: true
                                                    horizontalAlignment: Text.AlignHCenter
                                                }

                                            }

                                            background: Rectangle {
                                                color: easingButton.isCurrent ? root.palette.highlight : (easingButton.hovered ? root.textColor(0.1) : root.panelColor(0.55))
                                                border.color: easingButton.isCurrent ? root.palette.highlight : root.textColor(0.24)
                                                border.width: easingButton.isCurrent ? 2 : 1
                                                radius: 4
                                            }

                                        }

                                    }

                                }

                            }

                        }

                    }

                }

            }

        }

    }

}
