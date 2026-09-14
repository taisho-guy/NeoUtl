import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "./Components"

Window {
    id: launcherWindow
    width: 860
    height: 640
    minimumWidth: 720
    minimumHeight: 520
    title: "NeoUtl - プロジェクトランチャー"
    color: Theme.bg
    visible: true

    signal projectOpened(string dir, string name, int fps, int width, int height)
    signal createRequested(string name, int fps, int width, int height, int sampleRate, int channels)
    signal requestSystemSettings()

    property bool selectionMode: false
    property var selectedDirs: []
    property int sortIndex: 0
    readonly property var sortLabels: ["並べ替え：名前 ↑", "並べ替え：名前 ↓", "並べ替え：更新日時 ↑", "並べ替え：更新日時 ↓"]

        property var projectList: [
        { name: "Sample Project", dir: "/sample/path", width: 1920, height: 1080, fps: 30, modified: "2026-09-14 12:00" }
    ]

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

                NeoCard {
            Layout.fillWidth: true
            heading: "新規プロジェクト"

            RowLayout {
                width: parent.width
                spacing: 16

                ColumnLayout {
                    spacing: 4
                    Text { text: "映像"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                    RowLayout {
                        spacing: 8
                        NeoNumberInput { id: fpsInput; value: 30; from: 1; to: 240; suffix: "fps" }
                        NeoNumberInput { id: widthInput; value: 1920; from: 16; to: 7680; suffix: "px" }
                        NeoNumberInput { id: heightInput; value: 1080; from: 16; to: 7680; suffix: "px" }
                    }
                }

                Item { Layout.fillWidth: true }

                ColumnLayout {
                    spacing: 4
                    Text { text: "音声"; color: Theme.textMuted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                    RowLayout {
                        spacing: 8
                        NeoNumberInput { id: channelsInput; value: 2; from: 1; to: 8; suffix: "ch" }
                        NeoNumberInput { id: sampleRateInput; value: 48000; from: 8000; to: 192000; suffix: "Hz" }
                    }
                }
            }

            RowLayout {
                width: parent.width
                spacing: 8

                NeoTextInput {
                    id: projectNameInput
                    Layout.fillWidth: true
                    placeholderText: "プロジェクト名を入力…"
                    text: "新規プロジェクト"
                }

                NeoButton {
                    text: "作成して開く"
                    onClicked: {
                        launcherWindow.createRequested(
                            projectNameInput.text,
                            fpsInput.value,
                            widthInput.value,
                            heightInput.value,
                            sampleRateInput.value,
                            channelsInput.value
                        );
                        launcherWindow.projectOpened(
                            "",
                            projectNameInput.text,
                            fpsInput.value,
                            widthInput.value,
                            heightInput.value
                        );
                    }
                }
            }
        }

                RowLayout {
            Layout.fillWidth: true
            spacing: 8

            NeoButton {
                outline: true
                text: launcherWindow.sortLabels[launcherWindow.sortIndex]
                onClicked: {
                    launcherWindow.sortIndex = (launcherWindow.sortIndex + 1) % launcherWindow.sortLabels.length;
                }
            }

            Item { Layout.fillWidth: true }

            NeoButton {
                visible: launcherWindow.selectionMode
                outline: true
                danger: true
                text: "削除"
                enabled: launcherWindow.selectedDirs.length > 0
                onClicked: {
                    launcherWindow.selectedDirs = [];
                }
            }

            NeoButton {
                visible: launcherWindow.selectionMode
                outline: true
                text: "コピー"
                enabled: launcherWindow.selectedDirs.length > 0
            }

            NeoButton {
                outline: true
                text: launcherWindow.selectionMode ? "選択モードを出る" : "選択モードに入る"
                onClicked: {
                    launcherWindow.selectionMode = !launcherWindow.selectionMode;
                    if (!launcherWindow.selectionMode) {
                        launcherWindow.selectedDirs = [];
                    }
                }
            }
        }

                NeoTextInput {
            id: searchInput
            Layout.fillWidth: true
            placeholderText: "既存プロジェクトを検索…"
        }

                ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            ListView {
                id: projectListView
                width: parent.width
                spacing: 6
                model: launcherWindow.projectList

                delegate: Rectangle {
                    id: rowRect
                    width: projectListView.width
                    height: 48
                    radius: Theme.radiusSm
                    color: mouseArea.pressed ? Theme.cardHover :
                           mouseArea.containsMouse ? Qt.rgba(255, 255, 255, 0.05) : Theme.card
                    border.color: mouseArea.containsMouse ? Theme.textMuted : Theme.border
                    border.width: 1

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 12

                        CheckBox {
                            visible: launcherWindow.selectionMode
                            checked: launcherWindow.selectedDirs.indexOf(modelData.dir) !== -1
                            onCheckedChanged: {
                                var copy = launcherWindow.selectedDirs.slice();
                                var idx = copy.indexOf(modelData.dir);
                                if (checked && idx === -1) copy.push(modelData.dir);
                                else if (!checked && idx !== -1) copy.splice(idx, 1);
                                launcherWindow.selectedDirs = copy;
                            }
                        }

                        Text {
                            text: modelData.name
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: 14
                            font.bold: true
                        }

                        Item { Layout.fillWidth: true }

                        Text {
                            text: modelData.width + " × " + modelData.height + "  " + modelData.fps + "fps  ・  " + (modelData.modified || "")
                            color: Theme.textMuted
                            font.family: Theme.fontFamily
                            font.pixelSize: 12
                        }
                    }

                    MouseArea {
                        id: mouseArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: {
                            if (launcherWindow.selectionMode) {
                                                            } else {
                                launcherWindow.projectOpened(
                                    modelData.dir,
                                    modelData.name,
                                    modelData.fps,
                                    modelData.width,
                                    modelData.height
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
