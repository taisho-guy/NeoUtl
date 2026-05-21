import Qt.labs.platform as Platform
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import "common" as Common

ApplicationWindow {
    // Global Shortcuts

    id: mainWin

    // ショートカットの有効判定ヘルパー
    // 1. 入力系コントロール（TextField等）にフォーカスがある場合は無効化する
    readonly property bool _isInputFocused: {
        var item = Qt.application.focusItem;
        if (!item)
            return false;

        // フォーカスを持つアイテムがない場合は入力中ではない
        return item.hasOwnProperty("echoMode") || (item.hasOwnProperty("selectionStart") && item.readOnly === false);
    }

    function syncCompositeView() {
        // 修正: compositeViewLoader.item ではなく compositeView を直接渡す
        if (Workspace.currentTimeline)
            Workspace.currentTimeline.setCompositeView(compositeView);

    }

    // 全タブ横断で未保存確認し、全て処理済みになってから finalAction を実行
    function checkAllUnsavedAndExecute(finalAction) {
        if (!Workspace || !Workspace.tabs) {
            finalAction();
            return ;
        }
        for (var i = 0; i < Workspace.tabs.length; i++) {
            if (Workspace.tabs[i].hasUnsavedChanges) {
                // 対象タブをアクティブにしてダイアログを出す
                Workspace.currentIndex = i;
                saveConfirmDialog.pendingAction = function() {
                    // 保存/破棄が完了したら次の未保存タブへ進む
                    checkAllUnsavedAndExecute(finalAction);
                };
                saveConfirmDialog.open();
                return ; // ダイアログ完了を待つ（Cancelは pendingAction=null で自然停止）
            }
        }
        // 未保存タブが 0 なら即実行
        finalAction();
    }

    // 単一タブを対象にした確認（tabIndex が指定されたらそのタブをアクティブにする）
    function checkSaveAndExecute(action, tabIndex) {
        if (tabIndex !== undefined && Workspace.currentIndex !== tabIndex)
            Workspace.currentIndex = tabIndex;

        if (Workspace.currentTimeline && Workspace.currentTimeline.hasUnsavedChanges) {
            saveConfirmDialog.pendingAction = action;
            saveConfirmDialog.open();
        } else {
            action();
        }
    }

    // 修正: 有効なプロジェクト（タブ）が存在しない場合はメインウィンドウを非表示にする。
    // これにより、不必要なレンダリングコンテキストの生成とそれによるクラッシュを抑制する。
    visible: Workspace && Workspace.tabs ? Workspace.tabs.length > 0 : false
    width: 640
    height: 360
    x: 100
    y: 100
    objectName: "mainWindow"
    title: qsTr("AviQtl - プレビュー")
    onClosing: (close) => {
        // 一旦クローズをキャンセルし、全タブの未保存確認を行ってから終了する
        close.accepted = false;
        checkAllUnsavedAndExecute(function() {
            if (WindowManager)
                WindowManager.requestQuit();

        });
    }
    // 起動時に自分自身(Window)をコントローラーに渡す
    Component.onCompleted: {
        // 修正: visible プロパティを削除
        syncCompositeView();
    }

    // 末尾到達時に一時停止するだけのシンプルなロジック
    Connections {
        function onCurrentFrameChanged() {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport) {
                // totalFrames を排除し、動的に計算されるクリップの末尾（seekSlider.to）のみを基準にする。
                var limit = Math.floor(seekSlider.to) - 1;
                if (limit > 0 && Workspace.currentTimeline.transport.currentFrame >= limit)
                    Workspace.currentTimeline.transport.pause();

            }
        }

        target: Workspace.currentTimeline ? Workspace.currentTimeline.transport : null
    }

    FontLoader {
        source: "qrc:/resources/remixicon.ttf"
    }

    // アクション定義 (ショートカット用)
    Action {
        id: newAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.new"]) || "Ctrl+N"

        text: qsTr("新規プロジェクト")
        onTriggered: {
            // ランチャーで幅・高さ・fps を選ばせてから新規タブを作成する
            WindowManager.showLauncher();
        }
    }

    Action {
        id: saveProjectAction // プロジェクトの上書き保存用アクション

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.save"]) || "Ctrl+S"

        text: qsTr("プロジェクトの上書き保存")
        onTriggered: {
            if (Workspace.currentTimeline) {
                // 現在のプロジェクトパスが未設定の場合は名前を付けて保存ダイアログを開く
                if (Workspace.currentTimeline.currentProjectUrl === "")
                    saveDialog.open();
                else
                    Workspace.currentTimeline.saveProject("");
            }
        }
    }

    Action {
        id: loadAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.open"]) || "Ctrl+O"

        text: qsTr("プロジェクトを開く")
        onTriggered: {
            loadDialog.open();
        }
    }

    Action {
        id: saveAsProjectAction // プロジェクトを名前を付けて保存用アクション

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.saveAs"]) || "Ctrl+Shift+S"

        text: qsTr("プロジェクトを名前を付けて保存...")
        onTriggered: saveDialog.open()
    }

    Action {
        id: exportAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.export"]) || "Ctrl+E"

        text: qsTr("メディアの書き出し...")
        onTriggered: {
            exportDialog.x = mainWin.x + (mainWin.width - exportDialog.width) / 2;
            exportDialog.y = mainWin.y + (mainWin.height - exportDialog.height) / 2;
            exportDialog.open();
        }
    }

    Action {
        id: quitAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["app.quit"]) || "Ctrl+Q"

        text: qsTr("終了")
        onTriggered: {
            checkAllUnsavedAndExecute(function() {
                if (WindowManager)
                    WindowManager.requestQuit();

            });
        }
    }

    Action {
        id: systemSettingsAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["app.settings"]) || "Ctrl+P"

        text: qsTr("環境設定")
        onTriggered: {
            if (WindowManager)
                WindowManager.systemSettingsVisible = true;

        }
    }

    Action {
        id: projectSettingsAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["project.settings"]) || "Alt+Enter"

        text: qsTr("プロジェクト設定")
        onTriggered: {
            if (WindowManager)
                WindowManager.projectSettingsVisible = true;

        }
    }

    Action {
        id: showTimelineAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["view.timeline"]) || "F3"

        text: qsTr("タイムラインの表示")
        onTriggered: {
            if (WindowManager)
                WindowManager.timelineVisible = true;

        }
    }

    Action {
        id: showObjectSettingsAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["view.objectSettings"]) || "F4"

        text: qsTr("設定ダイアログの表示")
        onTriggered: {
            if (WindowManager)
                WindowManager.objectSettingsVisible = true;

        }
    }

    Action {
        id: addSceneAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.addScene"]) || "Ctrl+T"

        text: qsTr("新規シーン作成")
        onTriggered: {
            var win = WindowManager.getWindow("sceneSettings");
            if (win) {
                var count = Workspace.currentTimeline ? Workspace.currentTimeline.scenes.length : 0;
                win.openForCreate(qsTr("シーン %1").arg(count + 1));
            }
        }
    }

    Action {
        id: undoAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.undo"]) || "Ctrl+Z"

        text: qsTr("元に戻す")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.undo();

        }
    }

    Action {
        id: redoAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.redo"]) || "Ctrl+Shift+Z"

        text: qsTr("やり直す")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.redo();

        }
    }

    Action {
        id: playPauseAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["transport.playPause"]) || "Space"

        text: qsTr("再生 / 一時停止")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.togglePlay();

        }
    }

    Action {
        id: splitAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.split"]) || "S"

        text: qsTr("クリップを分割")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport) {
                var f = Workspace.currentTimeline.cursorFrame;
                if (Workspace.currentTimeline.selection && Workspace.currentTimeline.selection.selectedClipId >= 0) {
                    Workspace.currentTimeline.splitClip(Workspace.currentTimeline.selection.selectedClipId, f);
                } else {
                    if (Workspace.currentTimeline.selection && Workspace.currentTimeline.selection.selectedClipIds.length > 0) {
                        for (var i = 0; i < Workspace.currentTimeline.selection.selectedClipIds.length; i++) {
                            Workspace.currentTimeline.splitClip(Workspace.currentTimeline.selection.selectedClipIds[i], f);
                        }
                    }
                }
            }
        }
    }

    Action {
        id: currentSceneSettingsAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.sceneSettings"]) || "Alt+S"

        text: qsTr("現在のシーン設定...")
        onTriggered: {
            if (Workspace.currentTimeline) {
                var info = Workspace.currentTimeline.getSceneInfo(Workspace.currentTimeline.currentSceneId);
                var win = WindowManager.getWindow("sceneSettings");
                if (win && info)
                    win.openForScene(info.id, info.name, info.width, info.height, info.fps, info.totalFrames);

            }
        }
    }

    Action {
        id: removeCurrentSceneAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.removeScene"]) || "Ctrl+Shift+Delete"

        text: qsTr("現在のシーンを削除")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.currentSceneId !== 0)
                Workspace.currentTimeline.removeScene(Workspace.currentTimeline.currentSceneId);

        }
    }

    Action {
        id: toggleLayerLockAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.layerLock"]) || "Ctrl+L"

        text: qsTr("レイヤーロック切替")
        onTriggered: {
            if (Workspace.currentTimeline) {
                var l = Workspace.currentTimeline.selectedLayer;
                var isLocked = Workspace.currentTimeline.isLayerLocked(l);
                Workspace.currentTimeline.setLayerState(l, !isLocked, 0); // 0: Lock
            }
        }
    }

    Action {
        id: toggleLayerHideAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.layerHide"]) || "Ctrl+H"

        text: qsTr("レイヤー表示切替")
        onTriggered: {
            if (Workspace.currentTimeline) {
                var l = Workspace.currentTimeline.selectedLayer;
                var isHidden = Workspace.currentTimeline.isLayerHidden(l);
                Workspace.currentTimeline.setLayerState(l, !isHidden, 1); // 1: Hidden
            }
        }
    }

    Action {
        id: deleteAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.delete"]) || "Delete"

        text: qsTr("削除")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.deleteSelectedClips();

        }
    }

    Action {
        id: copyAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.copy"]) || "Ctrl+C"

        text: qsTr("コピー")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.copySelectedClips();

        }
    }

    Action {
        id: cutAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.cut"]) || "Ctrl+X"

        text: qsTr("カット")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.cutSelectedClips();

        }
    }

    Action {
        id: pasteAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.paste"]) || "Ctrl+V"

        text: qsTr("貼り付け")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport) {
                var f = Workspace.currentTimeline.cursorFrame;
                var l = Workspace.currentTimeline.selectedLayer !== undefined ? Workspace.currentTimeline.selectedLayer : 0;
                Workspace.currentTimeline.pasteClip(f, l);
            }
        }
    }

    Action {
        id: duplicateAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["edit.duplicate"]) || "Ctrl+D"

        text: qsTr("複製")
        onTriggered: {
            if (Workspace.currentTimeline) {
                Workspace.currentTimeline.copySelectedClips();
                var f = Workspace.currentTimeline ? Workspace.currentTimeline.cursorFrame : 0;
                var l = Workspace.currentTimeline.selectedLayer !== undefined ? Workspace.currentTimeline.selectedLayer : 0;
                Workspace.currentTimeline.pasteClip(f, l);
            }
        }
    }

    Action {
        id: nextFrameAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["transport.nextFrame"]) || "Right"

        text: qsTr("1フレーム進む")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.currentFrame = Math.min(Workspace.currentTimeline.transport.currentFrame + 1, Workspace.currentTimeline.transport.totalFrames);

        }
    }

    Action {
        id: prevFrameAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["transport.prevFrame"]) || "Left"

        text: qsTr("1フレーム戻る")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.currentFrame = Math.max(Workspace.currentTimeline.transport.currentFrame - 1, 0);

        }
    }

    Action {
        id: jumpStartAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["transport.jumpStart"]) || "Home"

        text: qsTr("先頭へ移動")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.currentFrame = 0;

        }
    }

    Action {
        id: jumpEndAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["transport.jumpEnd"]) || "End"

        text: qsTr("末尾へ移動")
        onTriggered: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.currentFrame = Workspace.currentTimeline.transport.totalFrames;

        }
    }

    Action {
        id: zoomInAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["view.zoomIn"]) || "Ctrl++"

        text: qsTr("ズームイン")
        onTriggered: {
            if (Workspace.currentTimeline) {
                var step = SettingsManager ? SettingsManager.value("timelineZoomStep", 10) : 10;
                var maxZ = SettingsManager ? SettingsManager.value("timelineZoomMax", 400) : 400;
                Workspace.currentTimeline.timelineScale = Math.min(Workspace.currentTimeline.timelineScale + step / 100, maxZ / 100);
            }
        }
    }

    Action {
        id: zoomOutAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["view.zoomOut"]) || "Ctrl+-"

        text: qsTr("ズームアウト")
        onTriggered: {
            if (Workspace.currentTimeline) {
                var step = SettingsManager ? SettingsManager.value("timelineZoomStep", 10) : 10;
                var minZ = SettingsManager ? SettingsManager.value("timelineZoomMin", 10) : 10;
                Workspace.currentTimeline.timelineScale = Math.max(Workspace.currentTimeline.timelineScale - step / 100, minZ / 100);
            }
        }
    }

    Action {
        id: moveUpAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.moveUp"]) || "Alt+Up"

        text: qsTr("レイヤーを上へ移動")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.moveSelectedClips(-1, 0);

        }
    }

    Action {
        id: moveDownAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.moveDown"]) || "Alt+Down"

        text: qsTr("レイヤーを下へ移動")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.moveSelectedClips(1, 0);

        }
    }

    Action {
        id: nudgeLeftAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.nudgeLeft"]) || "Alt+Left"

        text: qsTr("1フレーム左へ移動")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.moveSelectedClips(0, -1);

        }
    }

    Action {
        id: nudgeRightAction

        property string shortcutText: (SettingsManager.settings.shortcuts && SettingsManager.settings.shortcuts["timeline.nudgeRight"]) || "Alt+Right"

        text: qsTr("1フレーム右へ移動")
        onTriggered: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.moveSelectedClips(0, 1);

        }
    }

    Platform.MessageDialog {
        id: errorDialog

        title: qsTr("エラー")
        buttons: Platform.MessageDialog.Ok
    }

    Dialog {
        id: saveConfirmDialog

        property var pendingAction: null

        title: qsTr("保存の確認")
        x: (mainWin.width - width) / 2
        y: (mainWin.height - height) / 2
        modal: true
        parent: Overlay.overlay
        standardButtons: Dialog.Save | Dialog.Discard | Dialog.Cancel
        onAccepted: {
            var action = pendingAction;
            pendingAction = null; // 先にリセット（再入防止）
            if (Workspace.currentTimeline) {
                if (Workspace.currentTimeline.currentProjectUrl === "") {
                    // 名前を付けて保存 → saveDialog 完了後に action を実行
                    saveDialog._nextAction = action;
                    saveDialog.open();
                } else {
                    Workspace.currentTimeline.saveProject("");
                    if (action)
                        action();

                }
            }
        }
        onDiscarded: {
            var action = pendingAction;
            pendingAction = null;
            if (action)
                action();

        }
        onRejected: {
            pendingAction = null;
        }

        Label {
            text: qsTr("プロジェクトに保存されていない変更があります。\n続行する前に保存しますか？")
            wrapMode: Text.Wrap
        }

    }

    Connections {
        function onErrorOccurred(message) {
            errorDialog.text = message;
            errorDialog.open();
        }

        target: Workspace.currentTimeline
    }

    // FPSと再生速度の同期設定
    Binding {
        target: Workspace.currentTimeline ? Workspace.currentTimeline.transport : null
        property: "fps"
        value: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60
    }

    Connections {
        function onPlaybackSpeedChanged() {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.syncPlaybackSpeed();

        }

        target: Workspace.currentTimeline ? Workspace.currentTimeline.transport : null
    }

    Connections {
        function onSampleRateChanged() {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.updateAudioSampleRate();

        }

        target: Workspace.currentTimeline ? Workspace.currentTimeline.project : null
    }

    Connections {
        function onCurrentTimelineChanged() {
            syncCompositeView();
        }

        target: Workspace
    }

    Platform.FileDialog {
        id: saveDialog

        property var _nextAction: null

        title: qsTr("名前を付けて保存")
        fileMode: Platform.FileDialog.SaveFile
        nameFilters: ["AviQtl Project files (*.aviqtl)", "JSON files (*.json)"]
        defaultSuffix: "aviqtl"
        onAccepted: {
            if (Workspace.currentTimeline)
                Workspace.currentTimeline.saveProject(file);

            if (_nextAction)
                _nextAction();

            _nextAction = null;
        }
        onRejected: {
            _nextAction = null;
        }
    }

    Platform.FileDialog {
        id: loadDialog

        title: qsTr("プロジェクトを開く")
        nameFilters: ["AviQtl Project files (*.aviqtl)", "JSON files (*.json)"]
        onAccepted: {
            if (Workspace)
                Workspace.loadProject(file);

        }
    }

    ExportDialog {
        id: exportDialog

        ownerWindow: mainWin
    }

    // タブが 0 になったとき（最後のプロジェクトを閉じた時）にランチャーを自動表示
    Connections {
        function onTabsChanged() {
            if (Workspace.tabs.length === 0)
                WindowManager.showLauncher();

        }

        target: Workspace
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // プロジェクトタブバー（タブが 0 のときは非表示にして KDE TabBar の null アクセスを防ぐ）
        RowLayout {
            readonly property int _tabH: SettingsManager && SettingsManager.settings ? (SettingsManager.settings.timelineHeaderHeight || 28) : 28

            Layout.fillWidth: true
            visible: Workspace && Workspace.tabs && Workspace.tabs.length > 0
            Layout.preferredHeight: visible ? _tabH : 0
            Layout.minimumHeight: 0
            Layout.maximumHeight: visible ? _tabH : 0
            spacing: 0
            z: 1

            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff
                clip: true

                Loader {
                    active: Workspace && Workspace.tabs && Workspace.tabs.length > 0
                    width: parent ? parent.width : 0
                    height: parent ? parent.height : 0

                    sourceComponent: TabBar {
                        width: (parent && typeof parent.width !== "undefined" && parent.width > 0) ? Math.max(parent.width, contentWidth || 0) : 0

                        Repeater {
                            id: projectRepeater

                            model: Workspace ? Workspace.tabs : []

                            TabButton {
                                id: projectTabBtn

                                implicitWidth: Math.max(120, contentItem.implicitWidth + leftPadding + rightPadding)
                                checked: Workspace && Workspace.currentIndex === index
                                onClicked: {
                                    if (Workspace)
                                        Workspace.currentIndex = index;

                                }

                                contentItem: RowLayout {
                                    spacing: 4

                                    Text {
                                        text: modelData.name + (modelData.hasUnsavedChanges ? " *" : "")
                                        font: projectTabBtn.font
                                        color: palette.text
                                        horizontalAlignment: Text.AlignHCenter
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                        Layout.maximumWidth: 200
                                    }

                                    Button {
                                        flat: true
                                        Layout.preferredWidth: 20
                                        Layout.preferredHeight: 20
                                        hoverEnabled: true
                                        onClicked: {
                                            // 未保存確認後にタブを閉じる
                                            checkSaveAndExecute(function() {
                                                if (Workspace)
                                                    Workspace.closeProject(index);

                                            }, index);
                                        }

                                        contentItem: Common.AviQtlIcon {
                                            iconName: "close_line"
                                            size: 14
                                            color: parent.hovered ? parent.palette.highlight : parent.palette.text
                                        }

                                    }

                                }

                            }

                        }

                    }

                }

            }

            Button {
                flat: true
                Layout.preferredWidth: 40
                hoverEnabled: true
                Layout.fillHeight: true
                onClicked: {
                    // ランチャーで幅・高さ・fps を選ばせてから新規タブを作成する
                    WindowManager.showLauncher();
                }

                contentItem: Common.AviQtlIcon {
                    iconName: "add_line"
                    size: 16
                    color: parent.hovered ? parent.palette.highlight : parent.palette.text
                }

            }

        }

        CompositeView {
            id: compositeView

            // タイムラインの存在に依存するプロパティ
            sceneId: Workspace.currentTimeline ? Workspace.currentTimeline.currentSceneId : -1
            currentFrame: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Workspace.currentTimeline.transport.currentFrame : 0
            // 修正: clips プロパティへの依存を明示し、リアクティブなバインディングにする
            // C++ 側で clips プロパティの NOTIFY が呼ばれると、この式が再評価されます。
            clipModel: {
                var _trigger = Workspace.currentTimeline ? Workspace.currentTimeline.clips : null;
                if (Workspace.currentTimeline && sceneId >= 0) {
                    var clips = Workspace.currentTimeline.getSceneClips(sceneId);
                    // 3Dレンダラーの描画スタックにおいて、レイヤー番号が大きいほど手前に描画されるように
                    // モデルを降順でソートします。
                    return clips.sort((a, b) => {
                        return b.layer - a.layer;
                    });
                }
                return [];
            }
            Layout.fillWidth: true
            Layout.fillHeight: true
            onClipModelChanged: {
                console.log("[Debug] MainWindow -> CompositeView: clipModel property actually CHANGED. size:", clipModel ? clipModel.length : 0);
            }
            onSceneIdChanged: {
                console.log("[Debug] MainWindow -> CompositeView: sceneId changed to:", sceneId);
            }
            layerStates: {
                // WindowManager.timelineVisible を依存関係に含めることでタイムライン生成後に再評価させる
                var dummy = WindowManager.timelineVisible;
                var tlWin = WindowManager.getWindow("timeline");
                return tlWin ? tlWin.globalLayerStates : ({
                });
            }

            Connections {
                function onGlobalLayerStatesChanged() {
                    compositeView.layerStates = WindowManager.getWindow("timeline").globalLayerStates;
                }

                target: WindowManager.getWindow("timeline")
            }

            Connections {
                function onCurrentTimelineChanged() {
                    syncCompositeView();
                }

                target: Workspace
            }

        }

        // 再生コントロールバー
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 38
            color: mainWin.palette.window

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 10
                spacing: 10

                // シークバー
                Slider {
                    // 動かしている間は currentFrame を更新するがプレビューは止まる
                    // 離した瞬間にプレビュー確定
                    // 押した瞬間も同期
                    // シーク後は絶対に一時停止のままにするため、endScrub() は呼ばない

                    id: seekSlider

                    Layout.fillWidth: true
                    from: 0
                    to: {
                        // クリップの末尾を取得して動的に拡張
                        var maxEnd = 0;
                        var clipList = (Workspace.currentTimeline && Workspace.currentTimeline.clips) ? Workspace.currentTimeline.clips : [];
                        for (var j = 0; j < clipList.length; j++) {
                            var clip = clipList[j];
                            var end = clip.startFrame + clip.durationFrames;
                            if (end > maxEnd)
                                maxEnd = end;

                        }
                        return Math.max(1, maxEnd + 1);
                    }
                    onPressedChanged: {
                        if (Workspace.currentTimeline && Workspace.currentTimeline.transport) {
                            Workspace.currentTimeline.transport.isScrubbing = pressed;
                            if (pressed)
                                Workspace.currentTimeline.transport.beginScrub();
                            else
                                Workspace.currentTimeline.transport.setCurrentFrame_seek(Math.floor(value));
                        }
                    }
                    onMoved: {
                        if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                            Workspace.currentTimeline.transport.scrubTo(Math.floor(value));

                    }

                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                        cursorShape: seekSlider.pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    }

                    // スクラブ中はUI側からの書き換えを優先し、通常時はトランスポートに同期する
                    Binding on value {
                        when: !seekSlider.pressed
                        value: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Workspace.currentTimeline.transport.currentFrame : 0
                        restoreMode: Binding.RestoreBinding
                    }

                }

                Label {
                    // 総フレーム数の桁数に合わせて0埋めし、等幅フォントを適用する
                    text: {
                        var total = Math.floor(seekSlider.to);
                        var cur = Math.floor(seekSlider.value);
                        var digits = String(total).length;
                        // padStartを使用して先頭を0で埋める
                        return String(cur).padStart(digits, "0") + " / " + String(total);
                    }
                    font.family: "Monospace" // 等幅フォントの強制
                    font.features: {
                        "tnum": 1
                    } // OpenType tabular numerals
                    font.pixelSize: 12
                    color: mainWin.palette.text
                }

                // 操作ボタン
                RowLayout {
                    spacing: 0

                    Button {
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        flat: true
                        hoverEnabled: true
                        onClicked: {
                            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                                Workspace.currentTimeline.transport.setCurrentFrame_seek(Math.max(0, Workspace.currentTimeline.transport.currentFrame - 1));

                        }

                        contentItem: Common.AviQtlIcon {
                            iconName: "arrow_left_s_line"
                            size: 24
                            color: parent.hovered ? parent.palette.highlight : parent.palette.text
                        }

                    }

                    Button {
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        flat: true
                        onClicked: {
                            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                                Workspace.currentTimeline.transport.togglePlay();

                        }

                        contentItem: Common.AviQtlIcon {
                            iconName: (Workspace.currentTimeline && Workspace.currentTimeline.transport && Workspace.currentTimeline.transport.isPlaying) ? "pause_fill" : "play_fill"
                            size: 24
                            color: parent.hovered ? parent.palette.highlight : parent.palette.text
                        }

                    }

                    Button {
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        flat: true
                        onClicked: {
                            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                                Workspace.currentTimeline.transport.setCurrentFrame_seek(Workspace.currentTimeline.transport.currentFrame + 1);

                        }

                        contentItem: Common.AviQtlIcon {
                            iconName: "arrow_right_s_line"
                            size: 24
                            color: parent.hovered ? parent.palette.highlight : parent.palette.text
                        }

                    }

                }

                // 再生速度
                RowLayout {
                    spacing: 5

                    Label {
                        text: qsTr("速度")
                        color: mainWin.palette.text
                        font.pixelSize: 12
                    }

                    SpinBox {
                        from: SettingsManager ? SettingsManager.value("timelineZoomMin", 10) : 10
                        to: SettingsManager ? SettingsManager.value("timelineZoomMax", 400) : 400
                        stepSize: SettingsManager ? SettingsManager.value("timelineZoomStep", 10) : 10
                        editable: true
                        Layout.preferredWidth: 80
                        Layout.preferredHeight: 28
                        enabled: !(Workspace.currentTimeline && Workspace.currentTimeline.transport && Workspace.currentTimeline.transport.isPlaying)
                        // 値のバインディング
                        value: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Math.round(Workspace.currentTimeline.transport.playbackSpeed * 100) : 100
                        onValueModified: {
                            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                                Workspace.currentTimeline.transport.playbackSpeed = value / 100;

                        }
                        textFromValue: function(value, locale) {
                            return (value / 100).toFixed(1) + "x";
                        }
                        valueFromText: function(text, locale) {
                            return Number.fromLocaleString(locale, text.replace("x", "")) * 100;
                        }
                    }

                }

            }

        }

    }

    // Global Shortcuts
    Shortcut {
        sequence: newAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: newAction.trigger()
    }

    Shortcut {
        sequence: saveProjectAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: saveProjectAction.trigger()
    }

    Shortcut {
        sequence: loadAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: loadAction.trigger()
    }

    Shortcut {
        sequence: saveAsProjectAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: saveAsProjectAction.trigger()
    }

    Shortcut {
        sequence: exportAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: exportAction.trigger()
    }

    Shortcut {
        sequence: quitAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: quitAction.trigger()
    }

    Shortcut {
        sequence: systemSettingsAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: systemSettingsAction.trigger()
    }

    Shortcut {
        sequence: undoAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: undoAction.trigger()
    }

    Shortcut {
        sequence: redoAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: redoAction.trigger()
    }

    Shortcut {
        sequence: playPauseAction.shortcutText
        context: Qt.ApplicationShortcut
        // 再生・停止は入力中であっても（Spaceキー等）グローバルに効くのが一般的
        enabled: !_isInputFocused
        onActivated: playPauseAction.trigger()
    }

    Shortcut {
        sequence: splitAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: splitAction.trigger()
    }

    Shortcut {
        sequence: deleteAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: deleteAction.trigger()
    }

    Shortcut {
        sequence: copyAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: copyAction.trigger()
    }

    Shortcut {
        sequence: cutAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: cutAction.trigger()
    }

    Shortcut {
        sequence: pasteAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: pasteAction.trigger()
    }

    Shortcut {
        sequence: duplicateAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: duplicateAction.trigger()
    }

    Shortcut {
        sequence: nextFrameAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: nextFrameAction.trigger()
    }

    Shortcut {
        sequence: prevFrameAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: prevFrameAction.trigger()
    }

    Shortcut {
        sequence: jumpStartAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: jumpStartAction.trigger()
    }

    Shortcut {
        sequence: jumpEndAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: jumpEndAction.trigger()
    }

    Shortcut {
        sequence: zoomInAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: zoomInAction.trigger()
    }

    Shortcut {
        sequence: zoomOutAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: zoomOutAction.trigger()
    }

    Shortcut {
        sequence: moveUpAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: moveUpAction.trigger()
    }

    Shortcut {
        sequence: moveDownAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: moveDownAction.trigger()
    }

    Shortcut {
        sequence: nudgeLeftAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: nudgeLeftAction.trigger()
    }

    Shortcut {
        sequence: nudgeRightAction.shortcutText
        context: Qt.WindowShortcut
        enabled: !_isInputFocused
        onActivated: nudgeRightAction.trigger()
    }

    Shortcut {
        sequence: projectSettingsAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: projectSettingsAction.trigger()
    }

    Shortcut {
        sequence: showTimelineAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: showTimelineAction.trigger()
    }

    Shortcut {
        sequence: showObjectSettingsAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: showObjectSettingsAction.trigger()
    }

    Shortcut {
        sequence: addSceneAction.shortcutText
        context: Qt.ApplicationShortcut
        enabled: !_isInputFocused
        onActivated: addSceneAction.trigger()
    }

    // View3D の背後に黒背景を強制しない
    background: Rectangle {
        color: mainWin.palette.window
    }

    menuBar: MenuBar {
        // ─── ファイル ───
        Menu {
            title: qsTr("ファイル")

            Common.IconMenuItem {
                action: newAction
                iconName: "file_add_line"
            }

            Common.IconMenuItem {
                action: loadAction
                iconName: "folder_open_line"
            }

            MenuSeparator {
            }

            Common.IconMenuItem {
                action: saveProjectAction
                iconName: "save_line"
            }

            Common.IconMenuItem {
                action: saveAsProjectAction
                iconName: "save_3_line"
            }

            MenuSeparator {
            }

            Common.IconMenuItem {
                action: exportAction
                iconName: "movie_line"
            }

            MenuSeparator {
            }

            Common.IconMenuItem {
                text: qsTr("終了")
                action: quitAction
                iconName: "close_circle_line"
            }

        }

        // ─── 編集 ───
        Menu {
            title: qsTr("編集")

            Common.IconMenuItem {
                action: undoAction
                iconName: "arrow_go_back_line"
            }

            Common.IconMenuItem {
                action: redoAction
                iconName: "arrow_go_forward_line"
            }

        }

        // ─── 設定 ───
        Menu {
            title: qsTr("設定")

            Common.IconMenuItem {
                action: projectSettingsAction
                iconName: "settings_4_line"
            }

            MenuSeparator {
            }

            Common.IconMenuItem {
                action: systemSettingsAction
                iconName: "settings_3_line"
            }

        }

        // ─── 表示 ───
        Menu {
            title: qsTr("表示")

            Common.IconMenuItem {
                action: showTimelineAction
                iconName: "layout_bottom_line"
            }

            Common.IconMenuItem {
                action: showObjectSettingsAction
                iconName: "equalizer_line"
            }

        }

        // ─── ツール ───
        Menu {
            title: qsTr("ツール")

            Common.IconMenuItem {
                text: qsTr("パッケージマネージャー")
                iconName: "archive_line"
                enabled: true
                onTriggered: {
                    var win = WindowManager.getWindow("packageManager");
                    if (win) {
                        win.x = mainWin.x + (mainWin.width - win.width) / 2;
                        win.y = mainWin.y + (mainWin.height - win.height) / 2;
                        win.show();
                        win.raise();
                        win.requestActivate();
                    }
                }
            }

            MenuSeparator {
            }

            Common.IconMenuItem {
                text: qsTr("バージョン情報")
                iconName: "information_line"
                onTriggered: {
                    var win = WindowManager.getWindow("about");
                    if (win) {
                        win.x = mainWin.x + (mainWin.width - win.width) / 2;
                        win.y = mainWin.y + (mainWin.height - win.height) / 2;
                        win.show();
                        win.raise();
                        win.requestActivate();
                    }
                }
            }

        }

    }

}
