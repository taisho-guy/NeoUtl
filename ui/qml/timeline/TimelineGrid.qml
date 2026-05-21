import QtQuick

Canvas {
    id: timelineGrid

    property real contentX: 0
    property real contentY: 0
    property real gridInterval: 1
    property int layerCount: 128
    property int layerHeight: 30
    property real scale: Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1
    property var gridSettings: ({
        "mode": "Auto",
        "bpm": 120,
        "offset": 0,
        "interval": 10,
        "subdivision": 4
    })

    onContentXChanged: requestPaint()
    onContentYChanged: requestPaint()
    onGridIntervalChanged: requestPaint()
    onGridSettingsChanged: requestPaint()
    onScaleChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    anchors.fill: parent
    onPaint: {
        if (width < 1 || height <= 0 || scale <= 0)
            return ;

        var ctx = getContext("2d");
        // 境界の1px残りを防ぐため、クリア範囲を広めにとる
        ctx.clearRect(-1, -1, width + 2, height + 2);
        ctx.lineWidth = 1;
        // 水平区切り線と行の背景色
        var startY = contentY;
        for (var i = 0; i < layerCount; i++) {
            // 整数座標で描画
            var ly = Math.round(i * layerHeight - startY);
            if (ly < -layerHeight || ly > height)
                continue;

            // 行の背景を塗り分け（アクセシビリティ向上のための縞模様）
            // レイヤーヘッダーの濃淡順（偶数行が明るめ）に合わせます
            ctx.fillStyle = (i % 2 === 0) ? Qt.rgba(1, 1, 1, 0.02) : "transparent";
            ctx.fillRect(0, ly, width, layerHeight);
            // 水平区切り線
            ctx.strokeStyle = Qt.rgba(0.5, 0.5, 0.5, 0.2);
            ctx.beginPath();
            ctx.moveTo(0, ly);
            ctx.lineTo(width, ly);
            ctx.stroke();
        }
        if (!Workspace.currentTimeline)
            return ;

        // 垂直グリッド線
        var currentScale = scale;
        var currentContentX = contentX;
        var step = gridInterval;
        if (step <= 0)
            return ;

        var offsetF = (gridSettings.mode === "BPM" && Workspace.currentTimeline.project) ? gridSettings.offset * Workspace.currentTimeline.project.fps : 0;
        var isBpm = (gridSettings.mode === "BPM");
        var bpmDiv = isBpm ? (currentScale > 3 ? 4 : currentScale > 1.5 ? 2 : 1) : 1;
        var startN = Math.ceil((Math.floor(currentContentX / currentScale) - offsetF) / step);
        var endN = Math.floor((Math.ceil((currentContentX + width) / currentScale) - offsetF) / step);
        for (var n = startN; n <= endN; n++) {
            var f = offsetF + n * step;
            var x = f * currentScale - currentContentX;
            ctx.beginPath();
            if (isBpm) {
                var isMeasure = (n % (gridSettings.subdivision * bpmDiv) === 0);
                var isBeat = (n % bpmDiv === 0);
                if (isMeasure) {
                    ctx.strokeStyle = Qt.rgba(0.5, 0.8, 1, 0.5);
                    ctx.lineWidth = 1.5;
                } else if (isBeat) {
                    ctx.strokeStyle = Qt.rgba(0.5, 0.5, 0.5, 0.3);
                    ctx.lineWidth = 1;
                } else {
                    ctx.strokeStyle = Qt.rgba(0.5, 0.5, 0.5, 0.15);
                    ctx.lineWidth = 1;
                }
            } else {
                ctx.strokeStyle = Qt.rgba(0.5, 0.5, 0.5, 0.15);
                ctx.lineWidth = 1;
            }
            ctx.moveTo(x, 0);
            ctx.lineTo(x, height);
            ctx.stroke();
        }
    }

    Connections {
        function onCurrentTimelineChanged() {
            timelineGrid.requestPaint();
        }

        target: Workspace
    }

}
