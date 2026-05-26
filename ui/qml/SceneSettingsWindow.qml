import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common

Common.AviQtlWindow {
    id: root

    property bool isCreationMode: false // 新規作成モードか既存シーン編集モードか
    property int targetSceneId: -1

    // 新規シーン作成モードでダイアログを開く
    function openForCreate(defaultName) {
        isCreationMode = true; // 新規作成モード
        targetSceneId = -1; // 新規作成なのでIDは未定
        nameField.text = defaultName;
        // プロジェクトのデフォルト設定を初期値として使用
        if (Workspace.currentTimeline && Workspace.currentTimeline.project) {
            widthField.value = Workspace.currentTimeline.project.width;
            heightField.value = Workspace.currentTimeline.project.height;
            fpsField.value = Math.round(Workspace.currentTimeline.project.fps * 100);
        } else {
            widthField.value = SettingsManager ? SettingsManager.value("defaultProjectWidth", 1920) : 1920;
            heightField.value = SettingsManager ? SettingsManager.value("defaultProjectHeight", 1080) : 1080;
            fpsField.value = Math.round((SettingsManager ? SettingsManager.value("defaultProjectFps", 60) : 60) * 100);
        }
        // グリッド設定はデフォルト値
        modeCombo.currentIndex = 0;
        // Auto
        // Auto
        bpmField.text = "120";
        offsetField.text = "0";
        intervalField.text = "10";
        subdivisionField.text = "4";
        enableSnapCheck.checked = true;
        snapRangeField.value = 10;
        root.title = qsTr("新規シーン作成"); // タイトルを新規作成用に設定
        root.show();
        root.raise();
        root.requestActivate();
    }

    function openForScene(sceneId, name, w, h, fps, frames, gMode, gBpm, gOffset, gInterval, gSub, eSnap, mSnapRange) {
        isCreationMode = false; // 既存シーンの編集モード
        targetSceneId = sceneId;
        nameField.text = name;
        widthField.value = w;
        heightField.value = h;
        fpsField.value = Math.round(fps * 100);
        if (gMode === "BPM")
            modeCombo.currentIndex = 1;
        else if (gMode === "Frame")
            modeCombo.currentIndex = 2;
        else
            modeCombo.currentIndex = 0;
        bpmField.text = gBpm !== undefined ? gBpm : 120;
        offsetField.text = gOffset !== undefined ? gOffset : 0;
        intervalField.text = gInterval !== undefined ? gInterval : 10;
        subdivisionField.text = gSub !== undefined ? gSub : 4;
        enableSnapCheck.checked = eSnap !== undefined ? eSnap : true;
        snapRangeField.value = mSnapRange !== undefined ? mSnapRange : 10;
        root.show();
        root.raise();
        root.requestActivate();
    }

    title: isCreationMode ? qsTr("新規シーン作成") : qsTr("シーン設定")
    width: 450
    height: 550

    ScrollView {
        anchors.fill: parent
        anchors.margins: 15
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: parent.width
            spacing: 15

            GroupBox {
                title: qsTr("基本設定")
                Layout.fillWidth: true

                GridLayout {
                    columns: 2
                    rowSpacing: 10
                    columnSpacing: 10
                    anchors.fill: parent

                    Label {
                        text: qsTr("シーン名:")
                    }

                    TextField {
                        id: nameField

                        Layout.fillWidth: true
                        selectByMouse: true
                    }

                    Label {
                        text: qsTr("幅:")
                    }

                    SpinBox {
                        id: widthField

                        from: 1
                        to: SettingsManager ? SettingsManager.value("sceneWidthMax", 8000) : 8000
                        editable: true
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("高さ:")
                    }

                    SpinBox {
                        id: heightField

                        from: 1
                        to: 8000
                        editable: true
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("FPS:")
                    }

                    SpinBox {
                        id: fpsField

                        property real realValue: value / 100

                        from: SettingsManager ? SettingsManager.value("sceneFramesMin", 100) : 100
                        to: SettingsManager ? SettingsManager.value("sceneFramesMax", 24000) : 24000
                        stepSize: SettingsManager ? SettingsManager.value("sceneFramesStep", 100) : 100
                        editable: true
                        Layout.fillWidth: true
                        textFromValue: function(value, locale) {
                            return (value / 100).toFixed(2);
                        }
                        valueFromText: function(text, locale) {
                            return Number(text) * 100;
                        }
                    }

                }

            }

            GroupBox {
                title: qsTr("編集とスナップ")
                Layout.fillWidth: true

                GridLayout {
                    columns: 2
                    rowSpacing: 10
                    columnSpacing: 10
                    anchors.fill: parent

                    Label {
                        text: qsTr("スナップを有効にする:")
                    }

                    CheckBox {
                        id: enableSnapCheck

                        checked: true
                    }

                    Label {
                        text: qsTr("磁力スナップ範囲:")
                    }

                    SpinBox {
                        id: snapRangeField

                        from: 1
                        to: 100
                        editable: true
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("グリッドモード:")
                    }

                    ComboBox {
                        id: modeCombo

                        Layout.fillWidth: true
                        model: [qsTr("自動 (秒/フレーム)"), qsTr("BPM (音楽)"), qsTr("フレーム数固定")]
                    }

                }

            }

            GroupBox {
                title: qsTr("BPM設定")
                visible: modeCombo.currentIndex === 1
                Layout.fillWidth: true

                GridLayout {
                    columns: 2
                    rowSpacing: 10
                    columnSpacing: 10
                    anchors.fill: parent

                    Label {
                        text: qsTr("BPM:")
                    }

                    TextField {
                        id: bpmField

                        text: "120"
                        selectByMouse: true
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("拍子 (分割数):")
                    }

                    TextField {
                        id: subdivisionField

                        text: "4"
                        selectByMouse: true
                        Layout.fillWidth: true
                    }

                    Label {
                        text: qsTr("オフセット (秒):")
                    }

                    TextField {
                        id: offsetField

                        text: "0.0"
                        selectByMouse: true
                        Layout.fillWidth: true
                    }

                }

            }

            GroupBox {
                title: qsTr("フレーム設定")
                visible: modeCombo.currentIndex === 2
                Layout.fillWidth: true

                GridLayout {
                    columns: 2
                    rowSpacing: 10
                    columnSpacing: 10
                    anchors.fill: parent

                    Label {
                        text: qsTr("間隔 (Frames):")
                    }

                    TextField {
                        id: intervalField

                        text: "10"
                        selectByMouse: true
                        Layout.fillWidth: true
                    }

                }

            }

            Item {
                Layout.fillHeight: true
            }

            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10

                Button {
                    text: qsTr("キャンセル")
                    onClicked: root.hide()
                }

                Button {
                    text: "OK"
                    highlighted: true
                    onClicked: {
                        if (Workspace.currentTimeline) {
                            var framesToApply = isCreationMode ? (SettingsManager ? SettingsManager.value("defaultProjectFrames", 300) : 300) : Workspace.currentTimeline.getSceneDuration(targetSceneId);
                            var mKey = "Auto";
                            if (modeCombo.currentIndex === 1)
                                mKey = "BPM";

                            if (modeCombo.currentIndex === 2)
                                mKey = "Frame";

                            // 新規作成モードと編集モードで処理を分岐
                            if (isCreationMode) {
                                Workspace.currentTimeline.createScene(nameField.text);
                                // currentSceneId は既に新しいシーンのIDになっている
                                Workspace.currentTimeline.updateSceneSettings(Workspace.currentTimeline.currentSceneId, nameField.text, widthField.value, heightField.value, fpsField.realValue, framesToApply, mKey, parseFloat(bpmField.text) || 120, parseFloat(offsetField.text) || 0, parseInt(intervalField.text) || 10, parseInt(subdivisionField.text) || 4, enableSnapCheck.checked, snapRangeField.value);
                            } else if (targetSceneId !== -1) {
                                // 既存シーンの編集モードの場合
                                Workspace.currentTimeline.updateSceneSettings(targetSceneId, nameField.text, widthField.value, heightField.value, fpsField.realValue, framesToApply, mKey, parseFloat(bpmField.text) || 120, parseFloat(offsetField.text) || 0, parseInt(intervalField.text) || 10, parseInt(subdivisionField.text) || 4, enableSnapCheck.checked, snapRangeField.value);
                            }
                        }
                        root.hide();
                    }
                }

            }

        }

    }

}
