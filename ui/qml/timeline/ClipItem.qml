import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: clipDelegate

    property int layerHeight: 30
    property int layerCount: 128
    property int clipResizeHandleWidth: 6
    property Item flickableContentItem: null
    property var snapFrameFunc: function(f, ignoreSnap) {
        return f;
    }
    property int resizeDraftStart: -1
    property int resizeDraftDuration: -1
    property double scale: Workspace.currentTimeline ? Workspace.currentTimeline.timelineScale : 1
    property bool forceVisualSelection: false
    property var forcedSelectedIds: []
    readonly property bool committedSelected: ((Workspace.currentTimeline && Workspace.currentTimeline.selection) ? (Workspace.currentTimeline.selection.selectedClipIds.includes(modelData.id)) : false)
    readonly property bool previewSelected: Workspace.currentTimeline && Workspace.currentTimeline.previewSelectionIds ? Workspace.currentTimeline.previewSelectionIds.includes(modelData.id) : false
    readonly property bool forcedSelected: forceVisualSelection && forcedSelectedIds.includes(modelData.id)
    readonly property bool isSelected: previewSelected || (forceVisualSelection ? forcedSelected : committedSelected)
    readonly property bool isLayerLocked: getLayerLocked(modelData.layer)
    property int effectRevision: 0
    readonly property int controlLayerCount: {
        var _ = effectRevision;
        var effects = modelData.effectModels || [];
        for (var i = 0; i < effects.length; i++) {
            var eff = effects[i];
            if (!eff)
                continue;

            if (eff.id === "GroupControl" || eff.id === "camera_control" || eff.id === "camera") {
                var params = eff.params || {
                };
                return Math.max(0, Number(params.layerCount || 0));
            }
        }
        return Math.max(0, Number(modelData.groupLayerCount || 0));
    }
    property string clipDisplayName: (typeof modelData.name === "string" && modelData.name.length > 0) ? modelData.name : modelData.type
    property int dragDeltaStart: (isSelected && timelineViewRoot.isDraggingMulti) ? timelineViewRoot.activeDragDeltaFrame : 0
    property int dragDeltaLayer: (isSelected && timelineViewRoot.isDraggingMulti) ? timelineViewRoot.activeDragDeltaLayer : 0

    signal clipMoved(int clipId, int deltaLayer, int deltaStartFrame, int duration)
    signal clipResized(int clipId, int deltaStartFrame, int deltaDuration, int unused)
    signal clipDoubleClicked(int clipId)

    x: (resizeDraftStart >= 0 ? resizeDraftStart : Math.max(0, modelData.startFrame + dragDeltaStart)) * scale
    y: (Math.max(0, modelData.layer + dragDeltaLayer) + 0.1) * layerHeight
    width: (resizeDraftDuration >= 0 ? resizeDraftDuration : modelData.durationFrames) * scale
    height: layerHeight * 0.8
    z: modelData.layer

    Rectangle {
        visible: clipDelegate.controlLayerCount > 0
        x: 0
        y: parent.height
        width: parent.width
        height: clipDelegate.controlLayerCount * layerHeight
        color: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.18)
        border.color: Qt.rgba(palette.highlight.r, palette.highlight.g, palette.highlight.b, 0.55)
        border.width: 1
        z: -1
    }

    Connections {
        function onEffectParamChanged(clipId, effectIndex, paramName, value) {
            if (clipId === modelData.id && paramName === "layerCount")
                clipDelegate.effectRevision++;

        }

        target: Workspace.currentTimeline ?? null
    }

    Rectangle {
        id: clipRect

        readonly property var _rawColor: Workspace.currentTimeline ? Workspace.currentTimeline.getClipTypeColor(modelData.type) : ""
        readonly property color _baseColor: _rawColor !== "" ? _rawColor : Qt.darker(palette.highlight, 1.6)
        // テキストのコントラスト計算用に現在の実効的な背景色を保持
        readonly property color _displayColor: isSelected ? (_rawColor !== "" ? Qt.lighter(_rawColor, 1.3) : palette.highlight) : _baseColor

        anchors.fill: parent
        color: isSelected ? (_rawColor !== "" ? Qt.lighter(_rawColor, 1.3) : palette.highlight) : "transparent"
        gradient: isSelected ? null : clipGradient
        border.color: isSelected ? palette.highlight : palette.midlight
        border.width: isSelected ? 2 : 1
        opacity: 1
        radius: 0

        Gradient {
            id: clipGradient

            orientation: Gradient.Horizontal

            GradientStop {
                position: 0
                color: clipRect._baseColor
            }

            GradientStop {
                position: 1
                color: Qt.darker(clipRect._baseColor, 2)
            }

        }

        Text {
            readonly property int padding: 4
            property real stickyX: Math.max(0, (timelineViewRoot ? timelineViewRoot.contentX : 0) - clipDelegate.x)

            anchors.verticalCenter: parent.verticalCenter
            text: clipDelegate.clipDisplayName
            color: {
                if (isSelected && clipRect._rawColor === "")
                    return palette.highlightedText;

                // 背景の輝度（Luminance）に基づいて文字色を黒か白に切り替える
                var luma = (0.299 * clipRect._displayColor.r) + (0.587 * clipRect._displayColor.g) + (0.114 * clipRect._displayColor.b);
                return luma > 0.6 ? "#000000" : "#ffffff";
            }
            font.pixelSize: 10
            elide: Text.ElideRight
            x: Math.min(stickyX, parent.width - width - padding) + padding
            width: Math.min(implicitWidth, parent.width - padding * 2)
        }

        Canvas {
            id: waveformCanvas

            readonly property bool isAudio: modelData.type === "audio"
            property int waveRev: 0
            property bool _paintPending: false
            readonly property int displayDuration: {
                if (clipDelegate.resizeDraftDuration >= 0)
                    return clipDelegate.resizeDraftDuration;

                return modelData.durationFrames;
            }

            function _schedulePaint() {
                if (!isAudio)
                    return ;

                if (_paintPending)
                    return ;

                _paintPending = true;
                Qt.callLater(function() {
                    _paintPending = false;
                    waveformCanvas.requestPaint();
                });
            }

            anchors.fill: parent
            visible: isAudio
            opacity: 0.7
            onWidthChanged: _schedulePaint()
            onDisplayDurationChanged: _schedulePaint()
            onWaveRevChanged: _schedulePaint()
            onPaint: {
                var ctx = getContext("2d");
                ctx.clearRect(0, 0, width, height);
                if (!isAudio || width <= 0 || !Workspace.currentTimeline)
                    return ;

                var pw = Math.floor(width);
                var dur = displayDuration;
                if (pw <= 0 || dur <= 0)
                    return ;

                var peaks = Workspace.currentTimeline.getWaveformPeaks(modelData.id, pw, dur);
                if (!peaks || peaks.length === 0)
                    return ;

                var cy = height / 2;
                var maxH = cy - 2;
                ctx.strokeStyle = "rgba(255, 255, 255, 0.9)";
                ctx.lineWidth = 1;
                for (var i = 0; i < peaks.length / 2; i++) {
                    var pMin = peaks[i * 2];
                    var pMax = peaks[i * 2 + 1];
                    var yTop = cy - (pMax * maxH);
                    var yBottom = cy - (pMin * maxH);
                    // 振幅が極めて小さい場合でも1pxの線を表示する
                    if (yBottom - yTop < 1)
                        yBottom = yTop + 1;

                    ctx.beginPath();
                    ctx.moveTo(i + 0.5, yTop);
                    ctx.lineTo(i + 0.5, yBottom);
                    ctx.stroke();
                }
            }

            Connections {
                function onClipsChanged() {
                    waveformCanvas._schedulePaint();
                }

                target: Workspace.currentTimeline
            }

        }

        MouseArea {
            id: moveArea

            property int pressModifiers: 0
            property point dragStartScenePos: Qt.point(0, 0)
            property point pressScenePos: Qt.point(0, 0)
            property int initialLayer: 0
            property int initialFrame: 0
            property int lastProposedFrame: -1
            property int lastProposedLayer: -1
            property bool dragActive: false
            property real dragThreshold: 3
            property point lastScenePos: Qt.point(0, 0)
            property int lastModifiers: 0

            function updateDragFromScenePos(sp, modifiers) {
                var dX = sp.x - dragStartScenePos.x;
                var dY = sp.y - dragStartScenePos.y;
                var deltaFrame = Math.round(dX / clipDelegate.scale);
                var deltaLayer = Math.round(dY / layerHeight);
                var ignoreSnap = (modifiers & Qt.ShiftModifier);
                if (Workspace.currentTimeline && typeof Workspace.currentTimeline.resolveDragDelta === "function") {
                    var activeIds = (timelineViewRoot && timelineViewRoot.selectionVisualLatchIds) || [];
                    if (activeIds.length === 0 && Workspace.currentTimeline.selection)
                        activeIds = Workspace.currentTimeline.selection.selectedClipIds;

                    var res = Workspace.currentTimeline.resolveDragDelta(modelData.id, deltaFrame, deltaLayer, activeIds, timelineViewRoot.selectionMinFrame, timelineViewRoot.selectionMinLayer, timelineViewRoot.selectionMaxLayer, timelineViewRoot.layerCount);
                    var dF = res.x;
                    var dL = res.y;
                    if (!ignoreSnap)
                        dF = snapFrameFunc(initialFrame + dF, false) - initialFrame;

                    if (dF === timelineViewRoot.activeDragDeltaFrame && dL === timelineViewRoot.activeDragDeltaLayer)
                        return ;

                    timelineViewRoot.activeDragDeltaFrame = dF;
                    timelineViewRoot.activeDragDeltaLayer = dL;
                }
            }

            anchors.fill: parent
            anchors.leftMargin: clipResizeHandleWidth
            anchors.rightMargin: clipResizeHandleWidth
            acceptedButtons: Qt.LeftButton
            hoverEnabled: true
            cursorShape: clipDelegate.isLayerLocked ? Qt.ForbiddenCursor : (pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor)
            preventStealing: true
            onPressed: (mouse) => {
                timelineViewRoot.beginDragAutoScroll(function() {
                    if (dragActive)
                        updateDragFromScenePos(lastScenePos, lastModifiers);

                });
                if (clipDelegate.isLayerLocked)
                    return ;

                dragActive = false;
                pressModifiers = mouse.modifiers;
                // [FIX] REMOVED early selection logic here.
                // We rely on onReleased for both single-click and Ctrl-click,
                // to avoid state desync and input branching issues.
                // If dragging happens without selection, it will select it during drag or drop.
                // Wait! If dragging without selection, you drag an unselected clip?
                // The UX fix was exactly for that.
                // Let's bring it back BUT ONLY IF WE START DRAGGING!
                var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                pressScenePos = sp;
                dragStartScenePos = sp;
                initialLayer = modelData.layer;
                initialFrame = modelData.startFrame;
                lastProposedFrame = -1;
                lastProposedLayer = -1;
            }
            onPositionChanged: (mouse) => {
                if (!pressed || clipDelegate.isLayerLocked)
                    return ;

                var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                if (!dragActive) {
                    if (Math.abs(sp.x - pressScenePos.x) < dragThreshold && Math.abs(sp.y - pressScenePos.y) < dragThreshold)
                        return ;

                    dragActive = true;
                    if (!clipDelegate.isSelected && Workspace.currentTimeline)
                        Workspace.currentTimeline.handleClipClick(modelData.id, pressModifiers);

                    var minF = modelData.startFrame;
                    var minL = modelData.layer;
                    var maxL = modelData.layer;
                    if (Workspace.currentTimeline && Workspace.currentTimeline.selection) {
                        var ids = (timelineViewRoot && timelineViewRoot.selectionVisualLatchIds) || [];
                        if (ids.length === 0)
                            ids = Workspace.currentTimeline.selection.selectedClipIds;

                        if (ids.length > 0) {
                            minF = 1e+07;
                            minL = 1e+07;
                            maxL = -1;
                            for (var i = 0; i < Workspace.currentTimeline.clips.length; i++) {
                                var c = Workspace.currentTimeline.clips[i];
                                if (ids.includes(c.id)) {
                                    if (c.startFrame < minF)
                                        minF = c.startFrame;

                                    if (c.layer < minL)
                                        minL = c.layer;

                                    if (c.layer > maxL)
                                        maxL = c.layer;

                                }
                            }
                        }
                    }
                    timelineViewRoot.selectionMinFrame = minF;
                    timelineViewRoot.selectionMinLayer = minL;
                    timelineViewRoot.selectionMaxLayer = maxL;
                    timelineViewRoot.isDraggingMulti = true;
                    timelineViewRoot.activeDragDeltaFrame = 0;
                    timelineViewRoot.activeDragDeltaLayer = 0;
                }
                lastScenePos = sp;
                lastModifiers = mouse.modifiers;
                var vp = mapToItem(timelineViewRoot, mouse.x, mouse.y);
                timelineViewRoot.updateDragAutoScroll(vp);
                updateDragFromScenePos(sp, mouse.modifiers);
            }
            onReleased: (mouse) => {
                timelineViewRoot.endDragAutoScroll();
                if (!Workspace.currentTimeline)
                    return ;

                if (!dragActive) {
                    if (Workspace.currentTimeline)
                        Workspace.currentTimeline.handleClipClick(modelData.id, pressModifiers);

                    return ;
                }
                var deltaF = timelineViewRoot.activeDragDeltaFrame;
                var deltaL = timelineViewRoot.activeDragDeltaLayer;
                dragActive = false;
                // Emit relative movement FIRST so backend updates
                clipMoved(modelData.id, deltaL, deltaF, modelData.durationFrames);
                // Delay dropping the visual multi-drag state so that bindings
                // don't collapse before C++ model updates are fully propagated.
                var viewRoot = timelineViewRoot;
                Qt.callLater(function() {
                    if (viewRoot) {
                        viewRoot.isDraggingMulti = false;
                        viewRoot.activeDragDeltaFrame = 0;
                        viewRoot.activeDragDeltaLayer = 0;
                    }
                });
            }
            onDoubleClicked: {
                if (WindowManager)
                    WindowManager.raiseWindow("objectSettings");

            }
        }

        Rectangle {
            width: clipResizeHandleWidth
            height: parent.height
            anchors.left: parent.left
            color: "transparent"
            z: 10

            MouseArea {
                id: leftResizeArea

                property real startSceneX: 0 // Flickable contentItem 座標
                property int startFrame: 0
                property int startDuration: 0
                property bool resizing: false

                anchors.fill: parent
                cursorShape: clipDelegate.isLayerLocked ? Qt.ForbiddenCursor : Qt.SizeHorCursor
                hoverEnabled: true
                preventStealing: true
                onPressed: (mouse) => {
                    if (clipDelegate.isLayerLocked)
                        return ;

                    if (!clipDelegate.isSelected && Workspace.currentTimeline)
                        Workspace.currentTimeline.handleClipClick(modelData.id, mouse.modifiers);

                    var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                    startSceneX = sp.x;
                    startFrame = modelData.startFrame;
                    startDuration = modelData.durationFrames;
                    resizing = true;
                    mouse.accepted = true;
                }
                onPositionChanged: (mouse) => {
                    if (!resizing)
                        return ;

                    var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                    var delta = sp.x - startSceneX;
                    // 右端（終点）を固定して左端のみ動かす
                    var endFrame = startFrame + startDuration;
                    var rawNewStart = startFrame + delta / clipDelegate.scale;
                    var ignoreSnap = (mouse.modifiers & Qt.ShiftModifier);
                    var newStart = Math.max(0, snapFrameFunc(rawNewStart, ignoreSnap));
                    var newDur = endFrame - newStart;
                    var minDur = SettingsManager ? SettingsManager.value("minClipDurationFrames", 5) : 5;
                    if (newDur < minDur) {
                        newStart = endFrame - minDur;
                        newDur = minDur;
                    }
                    // ドラフトプロパティ経由で表示更新（バインディング破壊なし）
                    clipDelegate.resizeDraftStart = newStart;
                    clipDelegate.resizeDraftDuration = newDur;
                }
                onReleased: {
                    if (!resizing)
                        return ;

                    resizing = false;
                    if (Workspace.currentTimeline && clipDelegate.resizeDraftDuration > 0) {
                        var newStart = clipDelegate.resizeDraftStart >= 0 ? clipDelegate.resizeDraftStart : modelData.startFrame;
                        var deltaStart = newStart - startFrame;
                        var deltaDuration = clipDelegate.resizeDraftDuration - startDuration;
                        clipResized(modelData.id, deltaStart, deltaDuration, 0); // using params to pass delta
                    }
                    // ドラフト解除 → バインディングが自動で正値を返す
                    clipDelegate.resizeDraftStart = -1;
                    clipDelegate.resizeDraftDuration = -1;
                }
            }

        }

        Rectangle {
            width: clipResizeHandleWidth
            height: parent.height
            anchors.right: parent.right
            color: "transparent"
            z: 10

            MouseArea {
                id: rightResizeArea

                property real startSceneX: 0 // Flickable contentItem 座標
                property int startFrame: 0 // リサイズ開始時の startFrame
                property int startDuration: 0
                property bool resizing: false

                anchors.fill: parent
                cursorShape: clipDelegate.isLayerLocked ? Qt.ForbiddenCursor : Qt.SizeHorCursor
                hoverEnabled: true
                preventStealing: true
                onPressed: (mouse) => {
                    if (clipDelegate.isLayerLocked)
                        return ;

                    if (!clipDelegate.isSelected && Workspace.currentTimeline)
                        Workspace.currentTimeline.handleClipClick(modelData.id, mouse.modifiers);

                    var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                    startSceneX = sp.x;
                    startFrame = modelData.startFrame;
                    startDuration = modelData.durationFrames;
                    resizing = true;
                    mouse.accepted = true;
                }
                onPositionChanged: (mouse) => {
                    if (!resizing)
                        return ;

                    var sp = mapToItem(flickableContentItem, mouse.x, mouse.y);
                    var delta = sp.x - startSceneX;
                    // 右端フレームをスナップ
                    var rawEndFrame = startFrame + (startDuration * clipDelegate.scale + delta) / clipDelegate.scale;
                    var ignoreSnap = (mouse.modifiers & Qt.ShiftModifier);
                    var snappedEndFrame = snapFrameFunc(rawEndFrame, ignoreSnap);
                    var minDur = SettingsManager ? SettingsManager.value("minClipDurationFrames", 5) : 5;
                    var newDur = Math.max(minDur, snappedEndFrame - startFrame);
                    // ドラフトプロパティ経由で表示更新（バインディング破壊なし）
                    clipDelegate.resizeDraftDuration = newDur;
                }
                onReleased: {
                    if (!resizing)
                        return ;

                    resizing = false;
                    if (Workspace.currentTimeline && clipDelegate.resizeDraftDuration > 0) {
                        var deltaDuration = clipDelegate.resizeDraftDuration - startDuration;
                        clipResized(modelData.id, 0, deltaDuration, 0);
                    }
                    // ドラフト解除 → バインディングが自動で正値を返す
                    clipDelegate.resizeDraftDuration = -1;
                }
            }

        }

    }

}
