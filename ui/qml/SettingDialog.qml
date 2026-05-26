import Qt.labs.qmlmodels
import QtQml
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import "common" as Common

Common.AviQtlWindow {
    id: root

    property int targetClipId: (Workspace.currentTimeline && Workspace.currentTimeline.selection) ? Workspace.currentTimeline.selection.selectedClipId : -1
    property var effectsModel: []
    property var audioEffectsModel: []
    property bool inputting: false // 入力中フラグ（reloadループ防止用）
    property bool reloading: false
    property bool isDeleting: false // 複数エフェクト削除中フラグ（途中reload抑制用）
    property bool enableSnap: SettingsManager && SettingsManager.settings ? SettingsManager.settings.enableSnap : true
    property bool sidebarOnRight: (SettingsManager && SettingsManager.settings && SettingsManager.settings.settingDialogSidebarRight !== undefined) ? SettingsManager.settings.settingDialogSidebarRight : false
    readonly property real _projectFps: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60

    function currentSceneData() {
        if (!Workspace.currentTimeline || !Workspace.currentTimeline.scenes)
            return null;

        for (let i = 0; i < Workspace.currentTimeline.scenes.length; i++) {
            if (Workspace.currentTimeline.scenes[i].id === Workspace.currentTimeline.currentSceneId)
                return Workspace.currentTimeline.scenes[i];

        }
        return null;
    }

    function gridSettings() {
        const s = currentSceneData();
        if (s)
            return {
            "mode": s.gridMode || "Auto",
            "bpm": s.gridBpm || 120,
            "offset": s.gridOffset || 0,
            "interval": s.gridInterval || 10,
            "subdivision": s.gridSubdivision || 4
        };

        return {
            "mode": "Auto",
            "bpm": 120,
            "offset": 0,
            "interval": 10,
            "subdivision": 4
        };
    }

    function getGridInterval() {
        if (!Workspace.currentTimeline)
            return 1;

        const gs = gridSettings();
        const scale = Workspace.currentTimeline.timelineScale;
        const fps = (Workspace.currentTimeline.project && Workspace.currentTimeline.project.fps) ? Workspace.currentTimeline.project.fps : 60;
        if (gs.mode === "BPM") {
            const beatFrames = fps / (gs.bpm / 60);
            const bpmDiv = scale > 3 ? 4 : scale > 1.5 ? 2 : 1;
            return beatFrames / bpmDiv;
        }
        if (gs.mode === "Frame")
            return gs.interval;

        if (scale < 0.5)
            return Math.ceil(fps);

        if (scale < 1.5)
            return 10;

        return 1;
    }

    function snapRelativeFrame(relFrame) {
        if (!Workspace.currentTimeline || !enableSnap)
            return Math.max(0, Math.round(relFrame));

        const gs = gridSettings();
        const absFrame = Workspace.currentTimeline.clipStartFrame + relFrame;
        const step = getGridInterval();
        const offset = (gs.mode === "BPM" && Workspace.currentTimeline.project) ? gs.offset * Workspace.currentTimeline.project.fps : 0;
        const snappedAbs = Math.max(0, Math.round((Math.round((absFrame - offset) / step) * step) + offset));
        const newRelFrame = snappedAbs - Workspace.currentTimeline.clipStartFrame;
        // Don't snap outside the clip bounds if it goes negative
        return Math.max(0, newRelFrame);
    }

    function reload() {
        if (!Workspace.currentTimeline || !Workspace.currentTimeline.selection || reloading)
            return ;

        reloading = true;
        var id = Workspace.currentTimeline.selection.selectedClipId;
        var oldModel = sidebarList.model;
        var oldSelectedObjects = [];
        var oldCurrentObject = null;
        if (oldModel && sidebarList.selectedIndices) {
            for (var i = 0; i < sidebarList.selectedIndices.length; i++) {
                var idx = sidebarList.selectedIndices[i];
                if (idx >= 0 && idx < oldModel.length)
                    oldSelectedObjects.push(oldModel[idx]);

            }
            if (sidebarList.currentIndex >= 0 && sidebarList.currentIndex < oldModel.length)
                oldCurrentObject = oldModel[sidebarList.currentIndex];

        }
        if (id >= 0) {
            effectsModel = Workspace.currentTimeline.getClipEffectsModel(id);
            audioEffectsModel = Workspace.currentTimeline.getClipEffectStack(id);
        } else {
            effectsModel = [];
            audioEffectsModel = [];
        }
        // 保存したオブジェクト参照から新インデックスを復元
        var newModel = (Workspace.currentTimeline && Workspace.currentTimeline.isAudioClip(id)) ? audioEffectsModel : effectsModel;
        if (newModel && oldSelectedObjects.length > 0) {
            var newSel = [];
            for (var j = 0; j < newModel.length; j++) {
                if (oldSelectedObjects.indexOf(newModel[j]) !== -1)
                    newSel.push(j);

            }
            sidebarList.selectedIndices = newSel;
            var newCurrentIdx = newModel.indexOf(oldCurrentObject);
            if (newCurrentIdx !== -1)
                sidebarList.currentIndex = newCurrentIdx;
            else if (newSel.length > 0)
                sidebarList.currentIndex = newSel[newSel.length - 1];
        }
        reloading = false;
    }

    function executeEffectDelete(indices) {
        if (!Workspace.currentTimeline)
            return ;

        var isAudio = Workspace.currentTimeline.isAudioClip(targetClipId);
        var m = sidebarList.model;
        var toDelete = [];
        for (var i = 0; i < indices.length; i++) {
            var idx = indices[i];
            if (idx >= 0 && m && idx < m.length) {
                if (isAudio || (m[idx] && m[idx].kind === "effect"))
                    toDelete.push(idx);

            }
        }
        if (toDelete.length === 0) {
            sidebarList.clearSelection();
            return ;
        }
        toDelete.sort(function(a, b) {
            return b - a;
        });
        if (isAudio) {
            // AudioPlugin は clipsMutable ループ外で処理されるため従来方式
            root.isDeleting = true;
            for (var j = 0; j < toDelete.length; j++) {
                Workspace.currentTimeline.removeAudioPlugin(targetClipId, toDelete[j]);
            }
            root.isDeleting = false;
        } else {
            Workspace.currentTimeline.removeMultipleEffects(targetClipId, toDelete);
        }
        reload();
        sidebarList.clearSelection();
    }

    function scrollToEffect(index) {
        var isAudio = Workspace.currentTimeline && Workspace.currentTimeline.isAudioClip(targetClipId);
        var repeater = isAudio ? audioEffectsRepeater : videoEffectsRepeater;
        if (!repeater)
            return ;

        var item = repeater.itemAt(index);
        if (item) {
            // ターゲットのY座標を取得
            var targetY = item.y;
            // スクロール可能な最大値を計算 (コンテンツ全体の高さ - ビューポートの高さ)
            var maxScroll = Math.max(0, mainScrollView.contentHeight - mainScrollView.height);
            // ターゲット位置へワープ (0 ～ maxScroll の範囲に収める)
            mainScrollView.contentItem.contentY = Math.min(Math.max(0, targetY), maxScroll);
        }
    }

    function isEditableFocusItem(item) {
        if (!item)
            return false;

        return item.hasOwnProperty("echoMode") || (item.hasOwnProperty("selectionStart") && item.readOnly === false);
    }

    function clearInputFocusOutside(item, container, position) {
        if (!isEditableFocusItem(item) || !container || !position)
            return ;

        var localPos = item.mapFromItem(container, position.x, position.y);
        if (localPos.x < 0 || localPos.y < 0 || localPos.x > item.width || localPos.y > item.height)
            item.focus = false;

    }

    // UI定義を正規化してリストとして取得するヘルパー
    function getUiModel(effectModel) {
        if (!effectModel)
            return [];

        var ui = effectModel.uiDefinition;
        if (ui && ui.controls && typeof ui.controls.length === 'number')
            return ui.controls;

        console.warn("Invalid effect uiDefinition: ui.controls is missing for", effectModel ? effectModel.name : "unknown");
        return [];
    }

    width: 350
    height: 500
    title: qsTr("設定ダイアログ")
    color: palette.window
    visible: true
    x: 500
    y: 200
    onVisibleChanged: {
        if (visible)
            Qt.callLater(reload);

    }

    Connections {
        function onSelectedClipIdChanged() {
            reload();
        }

        function onSelectedClipDataChanged() {
            if (!inputting && !root.isDeleting)
                reload();

        }

        target: Workspace.currentTimeline ? Workspace.currentTimeline.selection : null
    }

    Connections {
        function onClipEffectsChanged(clipId) {
            if (clipId === targetClipId && !root.isDeleting)
                reload();

        }

        target: Workspace.currentTimeline
    }

    // タブ切り替えでプロジェクトが変わった際にモデルをリセットして再ロード
    Connections {
        function onCurrentTimelineChanged() {
            // 旧プロジェクトのサイドバー選択状態をクリア
            sidebarList.selectedIndices = [];
            sidebarList.currentIndex = -1;
            filterMenu._lastBuiltClipId = -2; // メニューキャッシュをリセット
            // エフェクトモデルを即座に空にしてから新プロジェクト向けに再ロード
            effectsModel = [];
            audioEffectsModel = [];
            Qt.callLater(reload);
        }

        target: Workspace
    }

    SplitView {
        id: settingsSplitView

        anchors.fill: parent
        orientation: Qt.Horizontal
        LayoutMirroring.enabled: root.sidebarOnRight
        LayoutMirroring.childrenInherit: true

        TapHandler {
            acceptedButtons: Qt.LeftButton
            gesturePolicy: TapHandler.WithinBounds
            onTapped: function(eventPoint) {
                root.clearInputFocusOutside(root.activeFocusItem, settingsSplitView, eventPoint.position);
            }
        }

        // エフェクト一覧サイドバー
        Rectangle {
            SplitView.preferredWidth: 200
            SplitView.minimumWidth: 150
            color: palette.midlight
            border.width: 1
            border.color: palette.mid

            ListView {
                id: sidebarList

                property int dragTargetIndex: -1
                property int dragSourceIndex: -1
                property var selectedIndices: []

                function toggleSelection(idx) {
                    var s = selectedIndices.slice();
                    var pos = s.indexOf(idx);
                    if (pos >= 0)
                        s.splice(pos, 1);
                    else
                        s.push(idx);
                    selectedIndices = s;
                    currentIndex = idx;
                }

                function rangeSelect(from, to) {
                    var s = [];
                    var lo = Math.min(from, to), hi = Math.max(from, to);
                    for (var i = lo; i <= hi; i++) s.push(i)
                    selectedIndices = s;
                    currentIndex = to;
                }

                function isSelected(idx) {
                    return selectedIndices.indexOf(idx) >= 0;
                }

                function clearSelection() {
                    selectedIndices = [];
                    currentIndex = -1;
                }

                anchors.fill: parent
                LayoutMirroring.enabled: false
                LayoutMirroring.childrenInherit: true
                clip: true
                // 選択中のクリップタイプに応じてモデルを切り替え
                model: (Workspace.currentTimeline && Workspace.currentTimeline.isAudioClip(targetClipId)) ? audioEffectsModel : effectsModel
                boundsBehavior: Flickable.StopAtBounds

                delegate: Item {
                    id: delegateRoot

                    width: sidebarList.width
                    height: 32
                    z: dragArea.drag.active ? 100 : 1

                    // ドロップ先を示すインジケーター（線）
                    Rectangle {
                        width: parent.width
                        height: 3
                        color: palette.highlight
                        visible: sidebarList.dragTargetIndex === index
                        z: 50
                        // ドラッグ元が自分より上なら下側に、下なら上側に線を引く
                        y: (sidebarList.dragSourceIndex < index) ? parent.height - height : 0
                    }

                    Item {
                        id: dragContainer

                        width: parent.width
                        height: parent.height

                        Rectangle {
                            anchors.fill: parent
                            color: (sidebarList.isSelected(index) || sidebarList.currentIndex === index) ? palette.highlight : (dragArea.drag.active ? palette.mid : "transparent")
                            opacity: (sidebarList.isSelected(index) || sidebarList.currentIndex === index) ? 0.2 : (dragArea.drag.active ? 0.8 : 1)
                        }

                        // 複数選択ドラッグ時のカウントバッジ
                        Rectangle {
                            visible: dragArea.drag.active && sidebarList.selectedIndices.length > 1 && sidebarList.isSelected(index)
                            width: 18
                            height: 18
                            radius: 9
                            color: palette.highlight
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 4
                            z: 10

                            Text {
                                anchors.centerIn: parent
                                text: sidebarList.selectedIndices.length
                                color: palette.highlightedText || "#ffffff"
                                font.pixelSize: 11
                                font.bold: true
                            }

                        }

                        // 背景用クリック領域（他のコントロールの下敷きになるよう先に宣言）
                        MouseArea {
                            anchors.fill: parent
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: (mouse) => {
                                if (mouse.button === Qt.RightButton) {
                                    if (!sidebarList.isSelected(index)) {
                                        sidebarList.selectedIndices = [index];
                                        sidebarList.currentIndex = index;
                                    }
                                    effectContextMenu.effectIndex = index;
                                    effectContextMenu.popup();
                                    return ;
                                }
                                if (mouse.modifiers & Qt.ControlModifier) {
                                    sidebarList.toggleSelection(index);
                                } else if (mouse.modifiers & Qt.ShiftModifier) {
                                    sidebarList.rangeSelect(sidebarList.currentIndex >= 0 ? sidebarList.currentIndex : 0, index);
                                } else {
                                    sidebarList.selectedIndices = [index];
                                    sidebarList.currentIndex = index;
                                }
                                root.scrollToEffect(index);
                            }
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 8
                            anchors.rightMargin: 8
                            spacing: 8

                            // ドラッグ用ハンドル
                            Common.AviQtlIcon {
                                iconName: "drag_move_line" // 適切なアイコン名に変更してください
                                size: 16
                                color: palette.text
                                opacity: 0.5

                                MouseArea {
                                    // 選択状態の追従は reload() 内のオブジェクト参照復元で行われます。

                                    id: dragArea

                                    anchors.fill: parent
                                    preventStealing: true
                                    cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                                    drag.target: dragContainer
                                    drag.axis: Drag.YAxis
                                    onPressed: {
                                        sidebarList.interactive = false;
                                        sidebarList.dragSourceIndex = index;
                                    }
                                    onPositionChanged: (mouse) => {
                                        if (drag.active) {
                                            // アイテムの中心Y座標を親のリスト基準で計算
                                            var absoluteY = delegateRoot.y + dragContainer.y + (dragContainer.height / 2);
                                            var hoverIndex = sidebarList.indexAt(10, absoluteY);
                                            if (hoverIndex !== -1 && hoverIndex !== index)
                                                sidebarList.dragTargetIndex = hoverIndex;
                                            else
                                                sidebarList.dragTargetIndex = -1;
                                        }
                                    }
                                    onReleased: (mouse) => {
                                        sidebarList.interactive = true;
                                        var targetIndex = sidebarList.dragTargetIndex;
                                        sidebarList.dragTargetIndex = -1;
                                        dragContainer.x = 0;
                                        dragContainer.y = 0;
                                        if (targetIndex === -1 || targetIndex === index)
                                            return ;

                                        var isAudio = Workspace.currentTimeline.isAudioClip(targetClipId);
                                        var sel = sidebarList.selectedIndices;
                                        // 複数選択中 かつ ドラッグ元が選択範囲内 → 一括移動
                                        if (!isAudio && sel.length > 1 && sel.indexOf(index) >= 0)
                                            Workspace.currentTimeline.reorderMultipleEffects(targetClipId, sel, targetIndex);
                                        else if (isAudio)
                                            Workspace.currentTimeline.reorderAudioPlugins(targetClipId, index, targetIndex);
                                        else
                                            Workspace.currentTimeline.reorderEffects(targetClipId, index, targetIndex);
                                    }
                                }

                            }

                            // 有効・無効切り替えチェックボックス
                            CheckBox {
                                checked: modelData.enabled !== undefined ? modelData.enabled : true
                                Layout.preferredHeight: 20
                                Layout.preferredWidth: 20
                                onToggled: (checked) => {
                                    if (!Workspace.currentTimeline)
                                        return ;

                                    var targets = (sidebarList.isSelected(index) && sidebarList.selectedIndices.length > 1) ? sidebarList.selectedIndices : [index];
                                    var isAudio = Workspace.currentTimeline.isAudioClip(targetClipId);
                                    for (var i = 0; i < targets.length; i++) {
                                        if (isAudio)
                                            Workspace.currentTimeline.setAudioPluginEnabled(targetClipId, targets[i], checked);
                                        else
                                            Workspace.currentTimeline.setEffectEnabled(targetClipId, targets[i], checked);
                                    }
                                }
                            }

                            // エフェクト名 (クリックでワープ)
                            Label {
                                text: modelData.name
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                color: palette.text
                            }

                        }

                    }

                }

            }

            Menu {
                id: effectContextMenu

                property int effectIndex: -1

                MenuItem {
                    text: {
                        var n = sidebarList.selectedIndices.length;
                        return n > 1 ? qsTr("選択した %1 件を削除").arg(n) : qsTr("削除");
                    }
                    enabled: {
                        var indices = sidebarList.selectedIndices.length > 0 ? sidebarList.selectedIndices : (effectContextMenu.effectIndex >= 0 ? [effectContextMenu.effectIndex] : []);
                        if (indices.length === 0)
                            return false;

                        var m = sidebarList.model;
                        if (!m)
                            return false;

                        var isAudio = Workspace.currentTimeline && Workspace.currentTimeline.isAudioClip(targetClipId);
                        for (var i = 0; i < indices.length; i++) {
                            var idx = indices[i];
                            if (idx >= 0 && idx < m.length)
                                if (isAudio || (m[idx] && m[idx].kind === "effect"))
                                return true;
;

                        }
                        return false;
                    }
                    onTriggered: {
                        if (!Workspace.currentTimeline)
                            return ;

                        var indices = sidebarList.selectedIndices.length > 0 ? sidebarList.selectedIndices.slice() : (effectContextMenu.effectIndex >= 0 ? [effectContextMenu.effectIndex] : []);
                        if (indices.length === 0)
                            return ;

                        root.executeEffectDelete(indices);
                    }
                }

            }

        }

        // 詳細設定スクロールビュー
        ScrollView {
            id: mainScrollView

            SplitView.fillWidth: true
            contentWidth: availableWidth
            clip: true

            ColumnLayout {
                width: parent.width
                spacing: 1
                layoutDirection: Qt.LeftToRight

                Repeater {
                    id: videoEffectsRepeater

                    model: effectsModel

                    delegate: ColumnLayout {
                        id: effectRoot

                        property int effectIndex: {
                            if (!Workspace.currentTimeline)
                                return index;

                            var resolvedIndex = Workspace.currentTimeline.getClipEffectIndex(targetClipId, modelData);
                            return resolvedIndex >= 0 ? resolvedIndex : index;
                        }
                        property var effectModel: modelData
                        property int _effectRev: 0

                        width: root.width
                        spacing: 0

                        Connections {
                            function onParamsChanged() {
                                effectRoot._effectRev++;
                            }

                            function onKeyframeTracksChanged() {
                                effectRoot._effectRev++;
                            }

                            target: effectRoot.effectModel
                            ignoreUnknownSignals: true
                        }

                        // エフェクトヘッダー
                        Rectangle {
                            Layout.fillWidth: true
                            height: 24
                            color: palette.midlight

                            Label {
                                text: modelData.name
                                color: palette.text
                                font.bold: true
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                            }

                            Button {
                                visible: modelData.kind === "effect" && !(effectRoot.effectIndex === 0 && modelData.id === "transform")
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                flat: true
                                hoverEnabled: true
                                width: 24
                                height: 24
                                onClicked: Workspace.currentTimeline.removeEffect(targetClipId, effectRoot.effectIndex)

                                contentItem: Common.AviQtlIcon {
                                    iconName: "close_line"
                                    size: 16
                                    color: parent.hovered ? "red" : parent.palette.text
                                }

                            }

                        }

                        // 全パラメータ（統一処理）
                        Repeater {
                            // 終了点は EasingConfigWindow が isEnd=true で生成する

                            model: getUiModel(effectModel)

                            delegate: ColumnLayout {
                                id: paramDelegate

                                property int activeDragOriginal: -1
                                property int activeDragCurrent: -1
                                property var def: modelData
                                property string key: (def && (def.param || def.name)) || ""
                                property var effVal: {
                                    var _ = effectRoot._effectRev;
                                    if (!effectModel)
                                        return undefined;

                                    var v = effectModel.evaluatedParam(key, curRelFrame, root._projectFps);
                                    if (v !== undefined && v !== null)
                                        return v;

                                    if (effectModel.params)
                                        return effectModel.params[key];

                                    return undefined;
                                }
                                property bool isNumber: typeof effVal === "number" && (!def.type || ["float", "number", "slider", "spinner", "int", "integer"].indexOf(def.type) !== -1)
                                property bool isColor: !!def && (def.type === "color" || def.type === "colour")
                                property bool supportsRangeUi: isNumber || isColor
                                property var effectModel: effectRoot.effectModel
                                property int effIdx: effectRoot.effectIndex
                                // キーフレーム
                                property int curRelFrame: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Math.max(0, Workspace.currentTimeline.transport.currentFrame - Workspace.currentTimeline.clipStartFrame) : 0
                                property int clipDur: Workspace.currentTimeline ? Workspace.currentTimeline.clipDurationFrames : 100
                                property var tracks: effectModel ? effectModel.keyframeTracks : null
                                property var rawKfs: {
                                    var _ = tracks;
                                    return effectModel ? effectModel.keyframeListForUi(key) : [];
                                }
                                property var kfs: keyframesWithVirtualEnd(rawKfs, clipDur)
                                property bool hasKeyframes: kfs.length > 0
                                property var interval: findInterval(kfs, curRelFrame, clipDur)
                                property int startFrame: interval.start
                                property int endFrame: interval.end
                                property var startVal: {
                                    var _t = tracks;
                                    var _r = effectRoot._effectRev;
                                    return effectModel ? effectModel.evaluatedParam(key, startFrame, root._projectFps) : effVal;
                                }
                                property var endVal: {
                                    var _t = tracks;
                                    var _r = effectRoot._effectRev;
                                    return effectModel ? effectModel.evaluatedParam(key, endFrame, root._projectFps) : effVal;
                                }
                                property string interpType: {
                                    var _ = tracks;
                                    return hasKeyframes ? getInterpAt(startFrame) : "constant";
                                }
                                property bool isMoving: supportsRangeUi && (hasKeyframes || interpType !== "constant")

                                function hasKeyframeAt(f) {
                                    if (!kfs)
                                        return false;

                                    for (var i = 0; i < kfs.length; i++) {
                                        if (kfs[i].frame === f)
                                            return true;

                                    }
                                    return false;
                                }

                                function hasRealKeyframeAt(f) {
                                    if (!rawKfs)
                                        return false;

                                    for (var i = 0; i < rawKfs.length; i++) {
                                        if (rawKfs[i].frame === f)
                                            return true;

                                    }
                                    return false;
                                }

                                function keyframesWithVirtualEnd(points, totalDur) {
                                    var out = [];
                                    if (points) {
                                        for (var i = 0; i < points.length; i++) out.push(points[i])
                                    }
                                    if (totalDur > 0) {
                                        var hasEnd = false;
                                        for (var j = 0; j < out.length; j++) {
                                            if (out[j].frame === totalDur) {
                                                hasEnd = true;
                                                break;
                                            }
                                        }
                                        if (!hasEnd) {
                                            var endValue = effectModel ? effectModel.evaluatedParam(key, totalDur, root._projectFps) : effVal;
                                            out.push({
                                                "frame": totalDur,
                                                "value": endValue,
                                                "interp": "none",
                                                "virtualEnd": true
                                            });
                                        }
                                    }
                                    out.sort(function(a, b) {
                                        return a.frame - b.frame;
                                    });
                                    return out;
                                }

                                function seekTrackFrameAt(xPos) {
                                    if (!Workspace.currentTimeline || !Workspace.currentTimeline.transport || clipDur <= 0 || trackItem.width <= 0)
                                        return ;

                                    var rawRelFrame = (xPos / trackItem.width) * clipDur;
                                    var relFrame = Math.max(0, Math.min(clipDur, Math.round(rawRelFrame)));
                                    Workspace.currentTimeline.transport.setCurrentFrame_seek(Workspace.currentTimeline.clipStartFrame + relFrame);
                                }

                                function ensureKeyframeAt(f) {
                                    if (!effectModel || !key)
                                        return ;

                                    if (hasRealKeyframeAt(f))
                                        return ;

                                    var raw = effectModel.evaluatedParam(key, f, root._projectFps);
                                    var v = (raw !== undefined && raw !== null) ? raw : effVal;
                                    Workspace.currentTimeline.setKeyframe(targetClipId, effIdx, paramDelegate.key, f, v, interpolationOptionsAt(f));
                                }

                                function ensureRangeKeyframes() {
                                    ensureKeyframeAt(startFrame);
                                    if (endFrame !== clipDur)
                                        ensureKeyframeAt(endFrame);

                                }

                                function findInterval(kfs, cur, totalDur) {
                                    let s = 0, e = totalDur;
                                    if (!kfs || kfs.length === 0)
                                        return {
                                        "start": s,
                                        "end": e
                                    };

                                    if (cur >= totalDur) {
                                        e = totalDur;
                                        for (let i = kfs.length - 1; i >= 0; i--) {
                                            if (kfs[i].frame < totalDur) {
                                                s = kfs[i].frame;
                                                break;
                                            }
                                        }
                                        return {
                                            "start": s,
                                            "end": e
                                        };
                                    }
                                    let foundStart = false;
                                    for (let i = kfs.length - 1; i >= 0; i--) {
                                        if (kfs[i].frame <= cur) {
                                            s = kfs[i].frame;
                                            foundStart = true;
                                            if (i + 1 < kfs.length)
                                                e = kfs[i + 1].frame;
                                            else
                                                e = totalDur;
                                            break;
                                        }
                                    }
                                    if (!foundStart) {
                                        e = kfs[0].frame;
                                        s = 0;
                                    }
                                    return {
                                        "start": s,
                                        "end": e
                                    };
                                }

                                function getInterpAt(f) {
                                    if (!kfs)
                                        return "linear";

                                    for (var i = 0; i < kfs.length; i++) {
                                        if (kfs[i].frame === f)
                                            return kfs[i].interp || "linear";

                                    }
                                    return "linear";
                                }

                                function interpolationOptionsAt(f) {
                                    var options = {
                                        "interp": "none"
                                    };
                                    if (!kfs)
                                        return options;

                                    var source = null;
                                    for (var i = kfs.length - 1; i >= 0; i--) {
                                        if (kfs[i].frame <= f) {
                                            source = kfs[i];
                                            break;
                                        }
                                    }
                                    if (!source && kfs.length > 0)
                                        source = kfs[0];

                                    if (!source)
                                        return options;

                                    options.interp = source.interp || "none";
                                    if (source.points)
                                        options.points = source.points;

                                    if (source.modeParams)
                                        options.modeParams = source.modeParams;

                                    return options;
                                }

                                function getGridLines() {
                                    if (!Workspace.currentTimeline || !enableSnap)
                                        return [];

                                    let step = getGridInterval();
                                    if (step <= 0)
                                        return [];

                                    let gs = gridSettings();
                                    let fps = (Workspace.currentTimeline.project && Workspace.currentTimeline.project.fps) ? Workspace.currentTimeline.project.fps : 60;
                                    let offsetF = (gs.mode === "BPM") ? gs.offset * fps : 0;
                                    let lines = [];
                                    let startAbs = Workspace.currentTimeline.clipStartFrame;
                                    let endAbs = startAbs + clipDur;
                                    let firstLine = Math.ceil((startAbs - offsetF) / step) * step + offsetF;
                                    if (clipDur / step > 150)
                                        return [];

                                    for (let f = firstLine; f <= endAbs; f += step) {
                                        let rel = f - startAbs;
                                        if (rel >= 0 && rel <= clipDur)
                                            lines.push(rel);

                                    }
                                    return lines;
                                }

                                function updateParam(frame, val) {
                                    if (!effectModel || !key)
                                        return ;

                                    if (!hasKeyframes) {
                                        Workspace.currentTimeline.updateClipEffectParam(targetClipId, effIdx, key, val);
                                        return ;
                                    }
                                    let type = "linear";
                                    if (frame === startFrame)
                                        type = getInterpAt(startFrame);
                                    else
                                        type = getInterpAt(frame);
                                    if (type === "constant")
                                        type = "linear";

                                    Workspace.currentTimeline.setKeyframe(targetClipId, effIdx, paramDelegate.key, frame, val, {
                                        "interp": type
                                    });
                                }

                                Layout.fillWidth: true
                                spacing: 0

                                // 数値
                                Common.ParamControl {
                                    Layout.fillWidth: true
                                    Layout.margins: 4
                                    visible: isNumber
                                    enabled: isNumber
                                    isRangeMode: isMoving && hasKeyframeAt(endFrame)
                                    interpolationType: interpType
                                    paramName: {
                                        var interpLabel = {
                                            "linear": qsTr(" (直線)"),
                                            "ease_in": qsTr(" (加速)"),
                                            "ease_out": qsTr(" (減速)"),
                                            "ease_in_out": qsTr(" (加減速)"),
                                            "bezier": qsTr(" (ベジェ)")
                                        };
                                        var name = (def.label && def.label !== "") ? def.label : key;
                                        return name + (isMoving ? (interpLabel[interpType] || "") : "");
                                    }
                                    startValue: Number(startVal) || 0
                                    endValue: Number(endVal) || 0
                                    minValue: (def.min !== undefined) ? def.min : ((key === "scale" || key === "opacity") ? 0 : -1000)
                                    maxValue: (def.max !== undefined) ? def.max : ((key === "scale") ? 500 : (key === "opacity" ? 1 : 1000))
                                    decimals: (def.decimals !== undefined) ? def.decimals : 2
                                    onStartValueModified: function(val) {
                                        root.inputting = true;
                                        updateParam(startFrame, val);
                                        var _rightActive = isMoving && hasKeyframeAt(endFrame) && interpType !== "" && interpType !== "constant";
                                        if (!_rightActive && endFrame !== startFrame && endFrame !== clipDur) {
                                            ensureKeyframeAt(endFrame);
                                            updateParam(endFrame, val);
                                        }
                                        root.inputting = false;
                                    }
                                    onEndValueModified: function(val) {
                                        root.inputting = true;
                                        updateParam(endFrame, val);
                                        root.inputting = false;
                                    }
                                    onParamButtonClicked: {
                                        if (!effectModel || !key)
                                            return ;

                                        // 区間キーフレームがない場合は生成
                                        ensureRangeKeyframes();
                                        var win = WindowManager.getWindow("easingConfig");
                                        if (win)
                                            win.openConfig({
                                            "clipId": targetClipId,
                                            "effectIndex": effIdx,
                                            "effectModel": effectModel,
                                            "paramName": key,
                                            "keyframeFrame": startFrame
                                        });

                                    }
                                }

                                // 非数値 (ControlLoader で型別UI)
                                Common.ControlLoader {
                                    property int startFrameState: startFrame
                                    property int endFrameState: endFrame
                                    property bool rightInteractiveState: isMoving && hasKeyframeAt(endFrame) && interpType !== "" && interpType !== "constant"

                                    Layout.fillWidth: true
                                    Layout.margins: 4
                                    visible: !isNumber
                                    enabled: true
                                    definition: def
                                    value: effVal
                                    effectRootRef: effectRoot
                                    onStartValueModified: function(val) {
                                        root.inputting = true;
                                        updateParam(startFrame, val);
                                        if (!rightInteractiveState && endFrame !== startFrame && endFrame !== clipDur) {
                                            ensureKeyframeAt(endFrame);
                                            updateParam(endFrame, val);
                                        }
                                        root.inputting = false;
                                    }
                                    onEndValueModified: function(val) {
                                        root.inputting = true;
                                        updateParam(endFrame, val);
                                        root.inputting = false;
                                    }
                                    onValueModified: function(val) {
                                        root.inputting = true;
                                        updateParam(startFrame, val);
                                        root.inputting = false;
                                    }
                                    onParamButtonClicked: {
                                        if (!effectModel || !key)
                                            return ;

                                        ensureRangeKeyframes();
                                        var win = WindowManager.getWindow("easingConfig");
                                        if (win)
                                            win.openConfig({
                                            "clipId": targetClipId,
                                            "effectIndex": effIdx,
                                            "effectModel": effectModel,
                                            "paramName": key,
                                            "keyframeFrame": startFrame
                                        });

                                    }
                                }

                                // ミニタイムラインバー
                                Item {
                                    id: trackItem

                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 12
                                    Layout.leftMargin: 4
                                    Layout.rightMargin: 4
                                    visible: supportsRangeUi

                                    Rectangle {
                                        anchors.centerIn: parent
                                        width: parent.width
                                        height: 2
                                        color: palette.mid

                                        Rectangle {
                                            property int vStart: (paramDelegate && paramDelegate.activeDragOriginal === startFrame) ? paramDelegate.activeDragCurrent : startFrame
                                            property int vEnd: (paramDelegate && paramDelegate.activeDragOriginal === endFrame) ? paramDelegate.activeDragCurrent : endFrame

                                            height: 4
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: palette.highlight
                                            opacity: 0.7
                                            x: (Math.min(vStart, vEnd) / clipDur) * parent.width
                                            width: Math.max(0, (Math.abs(vEnd - vStart) / clipDur) * parent.width)
                                            visible: clipDur > 0
                                        }

                                    }

                                    Repeater {
                                        model: enableSnap ? getGridLines() : []

                                        Rectangle {
                                            width: 1
                                            height: 8
                                            color: palette.midlight
                                            opacity: 0.6
                                            anchors.verticalCenter: parent.verticalCenter
                                            x: (modelData / clipDur) * trackItem.width
                                        }

                                    }

                                    Repeater {
                                        model: kfs

                                        Item {
                                            id: kfItem

                                            property int originalFrame: modelData.frame
                                            property int currentFrame: originalFrame
                                            // Capture outer scope variables here, where visual tree resolution works perfectly
                                            property var targetModel: effectModel
                                            property string targetKey: key
                                            property var rootWindow: root
                                            property int minDragFrame: 0
                                            property int maxDragFrame: clipDur
                                            property bool isEndpoint: originalFrame === 0 || !!modelData.virtualEnd

                                            width: 16
                                            height: 16
                                            anchors.verticalCenter: parent.verticalCenter
                                            x: Math.min(trackItem.width - width / 2, (currentFrame / clipDur) * trackItem.width - width / 2)

                                            Rectangle {
                                                width: 8
                                                height: 8
                                                color: kfMa.containsMouse || pointDrag.active ? palette.highlight : palette.text
                                                anchors.centerIn: parent
                                                rotation: 45
                                                antialiasing: true
                                            }

                                            MouseArea {
                                                id: kfMa

                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: kfItem.isEndpoint ? Qt.ArrowCursor : (pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor)
                                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                                onClicked: function(mouse) {
                                                    if (mouse.button === Qt.LeftButton)
                                                        paramDelegate.seekTrackFrameAt(kfItem.currentFrame / clipDur * trackItem.width);
                                                    else if (mouse.button === Qt.RightButton && !kfItem.isEndpoint)
                                                        Workspace.currentTimeline.removeKeyframe(targetClipId, effIdx, kfItem.targetKey, kfItem.originalFrame);
                                                }
                                                onDoubleClicked: function(mouse) {
                                                    mouse.accepted = true;
                                                }
                                            }

                                            DragHandler {
                                                id: pointDrag

                                                property real startX: 0

                                                target: null
                                                enabled: !kfItem.isEndpoint
                                                acceptedButtons: Qt.LeftButton
                                                onActiveChanged: {
                                                    if (active) {
                                                        startX = kfItem.x;
                                                        kfItem.rootWindow.inputting = true;
                                                        let minF = 0;
                                                        let maxF = clipDur;
                                                        for (let i = 0; i < kfs.length; i++) {
                                                            let f = kfs[i].frame;
                                                            if (f < kfItem.originalFrame && f >= minF)
                                                                minF = f + 1;

                                                            if (f > kfItem.originalFrame && f <= maxF)
                                                                maxF = f - 1;

                                                        }
                                                        kfItem.minDragFrame = minF;
                                                        kfItem.maxDragFrame = maxF;
                                                        if (typeof paramDelegate !== "undefined") {
                                                            paramDelegate.activeDragOriginal = kfItem.originalFrame;
                                                            paramDelegate.activeDragCurrent = kfItem.originalFrame;
                                                        }
                                                    } else {
                                                        if (typeof paramDelegate !== "undefined")
                                                            paramDelegate.activeDragOriginal = -1;

                                                        if (kfItem.currentFrame !== kfItem.originalFrame)
                                                            Workspace.currentTimeline.moveKeyframe(targetClipId, effIdx, kfItem.targetKey, kfItem.originalFrame, kfItem.currentFrame);

                                                        kfItem.rootWindow.inputting = false;
                                                    }
                                                }
                                                onTranslationChanged: {
                                                    if (active) {
                                                        let newX = startX + translation.x;
                                                        let rawRelFrame = ((newX + kfItem.width / 2) / trackItem.width) * clipDur;
                                                        let snappedFrame = snapRelativeFrame(rawRelFrame);
                                                        snappedFrame = Math.max(kfItem.minDragFrame, Math.min(kfItem.maxDragFrame, snappedFrame));
                                                        kfItem.currentFrame = snappedFrame;
                                                        if (typeof paramDelegate !== "undefined")
                                                            paramDelegate.activeDragCurrent = snappedFrame;

                                                    }
                                                }
                                            }

                                        }

                                    }

                                    Rectangle {
                                        width: 1
                                        height: parent.height
                                        color: palette.highlight
                                        x: (curRelFrame / clipDur) * parent.width
                                        visible: clipDur > 0
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        acceptedButtons: Qt.LeftButton
                                        onClicked: function(mouse) {
                                            seekTrackFrameAt(mouse.x);
                                        }
                                        onDoubleClicked: function(mouse) {
                                            let rawRelFrame = (mouse.x / trackItem.width) * clipDur;
                                            let f = snapRelativeFrame(rawRelFrame);
                                            f = Math.max(0, Math.min(clipDur, f));
                                            if (hasKeyframeAt(f))
                                                return ;

                                            let val = effectModel.evaluatedParam(key, f, root._projectFps);
                                            let options = interpolationOptionsAt(f);
                                            Workspace.currentTimeline.setKeyframe(targetClipId, effIdx, key, f, val, options);
                                        }
                                    }

                                }

                            }

                        }

                    }

                }

                // オーディオプラグインのパラメータ表示
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 1
                    visible: Workspace.currentTimeline && Workspace.currentTimeline.isAudioClip(targetClipId)

                    Repeater {
                        id: audioEffectsRepeater

                        model: audioEffectsModel

                        delegate: ColumnLayout {
                            id: audioEffectRoot

                            property int effectIndex: index

                            Layout.fillWidth: true
                            spacing: 0

                            Rectangle {
                                Layout.fillWidth: true
                                height: 24
                                color: palette.midlight

                                Label {
                                    text: modelData.name + " (" + modelData.format + ")"
                                    color: palette.text
                                    font.bold: true
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.left: parent.left
                                    anchors.leftMargin: 10
                                }

                                Button {
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    flat: true
                                    hoverEnabled: true
                                    width: 24
                                    height: 24
                                    onClicked: Workspace.currentTimeline.removeAudioPlugin(targetClipId, audioEffectRoot.effectIndex)

                                    contentItem: Common.AviQtlIcon {
                                        iconName: "close_line"
                                        size: 16
                                        color: parent.hovered ? "red" : parent.palette.text
                                    }

                                }

                            }

                            Repeater {
                                model: Workspace.currentTimeline.getEffectParameters(targetClipId, index)

                                delegate: Common.ControlLoader {
                                    Layout.fillWidth: true
                                    Layout.margins: 4
                                    definition: ({
                                        "type": modelData.type || "slider",
                                        "label": modelData.name,
                                        "min": modelData.min,
                                        "max": modelData.max
                                    })
                                    value: modelData.current
                                    onValueModified: (newValue) => {
                                        Workspace.currentTimeline.setEffectParameter(targetClipId, audioEffectRoot.effectIndex, modelData.pIdx, newValue);
                                    }
                                }

                            }

                        }

                    }

                }

            }

        }

        handle: Rectangle {
            implicitWidth: 4
            implicitHeight: 4
            color: splitMouseArea.pressed ? palette.highlight : palette.mid
            opacity: (splitMouseArea.pressed || splitMouseArea.containsMouse) ? 1 : 0

            MouseArea {
                id: splitMouseArea

                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
                cursorShape: Qt.SplitHCursor
            }

        }

    }

    // エフェクトサイドバー向け Delete ショートカット
    Shortcut {
        sequence: "Delete"
        context: Qt.WindowShortcut
        onActivated: {
            if (!Workspace.currentTimeline)
                return ;

            var indices = sidebarList.selectedIndices.length > 0 ? sidebarList.selectedIndices.slice() : (sidebarList.currentIndex >= 0 ? [sidebarList.currentIndex] : []);
            if (indices.length === 0)
                return ;

            root.executeEffectDelete(indices);
        }
    }

    menuBar: MenuBar {
        Menu {
            // ignore

            id: filterMenu

            property int _lastBuiltClipId: -2
            property var _dynamicObjects: []

            function _registerDynamic(obj) {
                if (obj)
                    _dynamicObjects.push(obj);

                return obj;
            }

            function _clearDynamicMenu() {
                for (var i = 0; i < _dynamicObjects.length; ++i) {
                    if (_dynamicObjects[i])
                        _dynamicObjects[i].destroy();

                }
                _dynamicObjects = [];
                while (filterMenu.count > 0)filterMenu.takeItem(0)
            }

            function buildMenu(parentMenu, items) {
                for (var i = 0; i < items.length; ++i) {
                    var node = items[i];
                    if (node.isCategory) {
                        var subMenu = _registerDynamic(subMenuComp.createObject(root.contentItem, {
                            "title": node.title
                        }));
                        buildMenu(subMenu, node.children);
                        parentMenu.addMenu(subMenu);
                    } else {
                        var effItem = _registerDynamic(menuItemComp.createObject(root.contentItem, {
                            "text": node.name,
                            "iconName": "magic_line"
                        }));
                        (function(id) {
                            effItem.triggered.connect(() => {
                                Workspace.currentTimeline.addEffect(targetClipId, id);
                            });
                        })(node.id);
                        parentMenu.addItem(effItem);
                    }
                }
            }

            title: qsTr("エフェクトを追加")
            onAboutToShow: {
                // 同じクリップに対してすでにメニューが構築されている場合は再構築をスキップ
                if (_lastBuiltClipId === targetClipId && filterMenu.count > 0)
                    return ;

                _clearDynamicMenu();
                _lastBuiltClipId = targetClipId;
                if (!Workspace.currentTimeline)
                    return ;

                if (targetClipId !== -1 && Workspace.currentTimeline.isAudioClip(targetClipId)) {
                    // オーディオプラグイン (VST等)
                    var categories = Workspace.currentTimeline.getPluginCategories();
                    for (var c = 0; c < categories.length; c++) {
                        var catName = categories[c];
                        var subMenu = _registerDynamic(subMenuComp.createObject(root.contentItem, {
                            "title": catName
                        }));
                        var plugins = Workspace.currentTimeline.getPluginsByCategory(catName);
                        for (var p = 0; p < plugins.length; p++) {
                            var plug = plugins[p];
                            var plugItem = _registerDynamic(menuItemComp.createObject(root.contentItem, {
                                "text": plug.name,
                                "iconName": "music_line"
                            }));
                            (function(id) {
                                plugItem.triggered.connect(() => {
                                    Workspace.currentTimeline.addAudioPlugin(targetClipId, id);
                                });
                            })(plug.id);
                            subMenu.addItem(plugItem);
                        }
                        filterMenu.addMenu(subMenu);
                    }
                } else {
                    // 標準エフェクト (木構造)
                    var effects = Workspace.currentTimeline.getAvailableEffects();
                    buildMenu(filterMenu, effects);
                }
            }

            Component {
                id: subMenuComp

                Menu {
                }

            }

            Component {
                id: menuItemComp

                Common.IconMenuItem {
                }

            }

        }

    }

}
