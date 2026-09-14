import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import "Dialogs"

ApplicationWindow {
    id: root
    visible: true
    width: 1280
    height: 800
    minimumWidth: 800
    minimumHeight: 600
    title: qsTr("NeoQtl — 高性能ゼロコピーQML動画編集ワークスペース")
    color: "#0d0f17"

        property int currentFrame: 0
    property int totalFrames: 1800
    property int fps: 60
    property bool isPlaying: false

        Timer {
        id: playbackTimer
        interval: 1000 / root.fps
        running: root.isPlaying
        repeat: true
        onTriggered: {
            if (root.currentFrame >= root.totalFrames) {
                root.currentFrame = 0;
            } else {
                root.currentFrame += 1;
            }
        }
    }

        menuBar: MenuBar {
        background: Rectangle { color: "#161924"; border.color: "#252b3d" }

        Menu {
            title: qsTr("ファイル(&F)")
            Action { text: qsTr("新規プロジェクト (&N)"); shortcut: "Ctrl+N"; onTriggered: root.currentFrame = 0 }
            Action { text: qsTr("プロジェクトを開く... (&O)"); shortcut: "Ctrl+O" }
            Action { text: qsTr("上書き保存 (&S)"); shortcut: "Ctrl+S" }
            Action { text: qsTr("名前を付けて保存... (&A)"); shortcut: "Ctrl+Shift+S" }
            MenuSeparator {}
            Action { text: qsTr("エクスポート... (&E)"); shortcut: "Ctrl+E"; onTriggered: exportDialog.open() }
            MenuSeparator {}
            Action { text: qsTr("終了 (&X)"); shortcut: "Ctrl+Q"; onTriggered: Qt.quit() }
        }

        Menu {
            title: qsTr("編集(&E)")
            Action { text: qsTr("元に戻す (&Z)"); shortcut: "Ctrl+Z" }
            Action { text: qsTr("やり直す (&Y)"); shortcut: "Ctrl+Y" }
            MenuSeparator {}
            Action { text: qsTr("クリップを切り取り (&T)"); shortcut: "Ctrl+X" }
            Action { text: qsTr("クリップをコピー (&C)"); shortcut: "Ctrl+C" }
            Action { text: qsTr("クリップを貼り付け (&V)"); shortcut: "Ctrl+V" }
        }

        Menu {
            title: qsTr("表示(&V)")
            Action {
                text: root.isPlaying ? qsTr("一時停止 (&P)") : qsTr("再生 (&P)")
                shortcut: "Space"
                onTriggered: root.isPlaying = !root.isPlaying
            }
            Action { text: qsTr("先頭フレームへ (&H)"); shortcut: "Home"; onTriggered: root.currentFrame = 0 }
            Action { text: qsTr("末尾フレームへ (&End)"); shortcut: "End"; onTriggered: root.currentFrame = root.totalFrames }
        }

        Menu {
            title: qsTr("設定(&S)")
            MenuItem { text: qsTr("プロジェクト設定... (&P)"); onTriggered: projDialog.open() }
            MenuItem { text: qsTr("システム設定... (&S)"); onTriggered: sysDialog.open() }
        }

        Menu {
            title: qsTr("ヘルプ(&H)")
            MenuItem { text: qsTr("バージョン情報 (&A)"); onTriggered: aboutDialog.open() }
        }
    }

        SplitView {
        anchors.fill: parent
        orientation: Qt.Vertical

                SplitView {
            SplitView.fillWidth: true
            SplitView.preferredHeight: 480
            orientation: Qt.Horizontal

                        PreviewPanel {
                id: previewPanel
                SplitView.fillWidth: true
                SplitView.preferredWidth: 880
                currentFrame: root.currentFrame
                totalFrames: root.totalFrames
                fps: root.fps
                isPlaying: root.isPlaying

                onPlayToggled: root.isPlaying = !root.isPlaying
                onFrameSeekRequested: root.currentFrame = frame
                onStepFrameRequested: {
                    root.currentFrame = Math.max(0, Math.min(root.totalFrames, root.currentFrame + delta));
                }
            }

                        PropertiesPanel {
                id: propertiesPanel
                SplitView.preferredWidth: 340
                SplitView.minimumWidth: 260
                SplitView.fillHeight: true

                onAddEffectRequested: effectDialog.open()
            }
        }

                TimelinePanel {
            id: timelinePanel
            SplitView.fillWidth: true
            SplitView.preferredHeight: 260
            SplitView.minimumHeight: 180
            currentFrame: root.currentFrame
            totalFrames: root.totalFrames
            fps: root.fps

            onFrameSeekRequested: root.currentFrame = frame
            onClipSelected: {
                propertiesPanel.clipId = clipId;
                propertiesPanel.clipName = name;
                propertiesPanel.clipKind = kind;
            }
        }
    }

        ProjectSettingsDialog {
        id: projDialog
        projFps: root.fps
        projTotalFrames: root.totalFrames
        onAccepted: {
            root.fps = projFps;
            root.totalFrames = projTotalFrames;
        }
    }

    SystemSettingsDialog {
        id: sysDialog
    }

    ExportDialog {
        id: exportDialog
    }

    EffectAddDialog {
        id: effectDialog
        onEffectSelected: {
            var curr = propertiesPanel.activeEffects.slice();
            curr.push({ id: effectId, name: effectName });
            propertiesPanel.activeEffects = curr;
        }
    }

    Dialog {
        id: aboutDialog
        title: qsTr("NeoQtl について")
        anchors.centerIn: parent
        width: 400; height: 220
        modal: true
        standardButtons: Dialog.Ok
        background: Rectangle { color: "#1e2230"; border.color: "#32384e"; radius: 8 }

        contentItem: ColumnLayout {
            spacing: 12
            Label {
                text: "NeoQtl (wgpu + Qt Quick / QML AOT)"
                font.bold: true; font.pixelSize: 16; color: "#6c8cff"
            }
            Label {
                text: qsTr("NeoUtl のすべてのロジックを継承し、Qt Quick (QML) によるリッチで高速なGUIを実現した次世代動画編集プロトタイプです。")
                wrapMode: Text.WordWrap; Layout.fillWidth: true; color: "#cbd5e1"
            }
            Label {
                text: "wgpu ゼロコピー RHI 統合プレビュー開通済"
                color: "#10b981"; font.pixelSize: 12
            }
        }
    }
}
