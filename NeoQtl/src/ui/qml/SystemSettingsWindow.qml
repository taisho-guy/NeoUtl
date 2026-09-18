import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "common" as Common
import "settings"

Common.AviQtlWindow {
    id: root

    property var draftSettings: ({
    })
    property alias currentTabIndex: tabBar.currentIndex
    property var pluginFormats: ["LADSPA", "DSSI", "LV2", "VST2", "VST3", "CLAP", "SF2", "SFZ", "JSFX", "Effects", "Objects"]
    property var themeValues: ["Dark", "Light", "System"]
    property var themeLabels: [qsTr("ダーク"), qsTr("ライト"), qsTr("システムに従う")]
    property var timeUnitValues: ["frame", "second"]
    property var timeUnitLabels: [qsTr("フレーム"), qsTr("秒")]
    property var renderThreadValues: [0, 1, 2, 4, 8, 16]
    property var renderThreadLabels: [qsTr("自動"), "1", "2", "4", "8", "16"]
    property var audioChannelValues: [1, 2]
    property var audioChannelLabels: [qsTr("モノラル"), qsTr("ステレオ")]
    property var blockSizeValues: [256, 512, 1024, 2048, 4096, 8192]
    property var videoCodecValues: ["h264_vaapi", "hevc_vaapi", "libx264"]
    property var videoCodecLabels: [qsTr("H.264 (VAAPI)"), qsTr("HEVC (VAAPI)"), qsTr("H.264 (CPU)")]
    property var audioCodecValues: ["aac", "opus", "flac", "pcm_s16le"]
    property var audioCodecLabels: ["AAC", "Opus", "FLAC", "PCM 16bit"]
    readonly property color secondaryTextColor: Qt.rgba(palette.text.r, palette.text.g, palette.text.b, 0.7)
    property var shortcutList: [{
        "id": "project.new",
        "name": qsTr("新規プロジェクト")
    }, {
        "id": "project.open",
        "name": qsTr("プロジェクトを開く")
    }, {
        "id": "project.save",
        "name": qsTr("上書き保存")
    }, {
        "id": "project.saveAs",
        "name": qsTr("名前を付けて保存")
    }, {
        "id": "project.export",
        "name": qsTr("メディアの書き出し")
    }, {
        "id": "app.quit",
        "name": qsTr("終了")
    }, {
        "id": "app.settings",
        "name": qsTr("環境設定")
    }, {
        "id": "edit.undo",
        "name": qsTr("元に戻す")
    }, {
        "id": "edit.redo",
        "name": qsTr("やり直す")
    }, {
        "id": "edit.cut",
        "name": qsTr("カット")
    }, {
        "id": "edit.copy",
        "name": qsTr("コピー")
    }, {
        "id": "edit.paste",
        "name": qsTr("貼り付け")
    }, {
        "id": "edit.delete",
        "name": qsTr("削除")
    }, {
        "id": "edit.duplicate",
        "name": qsTr("複製")
    }, {
        "id": "transport.playPause",
        "name": qsTr("再生 / 一時停止")
    }, {
        "id": "transport.nextFrame",
        "name": qsTr("1フレーム進む")
    }, {
        "id": "transport.prevFrame",
        "name": qsTr("1フレーム戻る")
    }, {
        "id": "transport.jumpStart",
        "name": qsTr("先頭へ移動")
    }, {
        "id": "transport.jumpEnd",
        "name": qsTr("末尾へ移動")
    }, {
        "id": "view.zoomIn",
        "name": qsTr("ズームイン")
    }, {
        "id": "view.zoomOut",
        "name": qsTr("ズームアウト")
    }, {
        "id": "view.timeline",
        "name": qsTr("タイムラインの表示")
    }, {
        "id": "view.objectSettings",
        "name": qsTr("設定ダイアログの表示")
    }, {
        "id": "project.settings",
        "name": qsTr("プロジェクト設定")
    }, {
        "id": "timeline.split",
        "name": qsTr("クリップを分割")
    }, {
        "id": "timeline.moveUp",
        "name": qsTr("レイヤーを上へ移動")
    }, {
        "id": "timeline.moveDown",
        "name": qsTr("レイヤーを下へ移動")
    }, {
        "id": "timeline.nudgeLeft",
        "name": qsTr("1フレーム左へ移動")
    }, {
        "id": "timeline.nudgeRight",
        "name": qsTr("1フレーム右へ移動")
    }, {
        "id": "timeline.addScene",
        "name": qsTr("新規シーン作成")
    }, {
        "id": "timeline.sceneSettings",
        "name": qsTr("現在のシーン設定")
    }, {
        "id": "timeline.removeScene",
        "name": qsTr("現在のシーンを削除")
    }, {
        "id": "timeline.layerLock",
        "name": qsTr("現在のレイヤーをロック/解除")
    }, {
        "id": "timeline.layerHide",
        "name": qsTr("現在のレイヤーを表示/非表示")
    }]

    function getShortcutValue(actionId, fallback) {
        if (!draftSettings["shortcuts"])
            return fallback;

        return draftSettings["shortcuts"][actionId] !== undefined ? draftSettings["shortcuts"][actionId] : fallback;
    }

    function setShortcutValue(actionId, value) {
        var next = cloneSettings(draftSettings);
        if (!next["shortcuts"])
            next["shortcuts"] = {
        };

        next["shortcuts"][actionId] = value;
        draftSettings = next;
    }

    function cloneSettings(source) {
        return JSON.parse(JSON.stringify(source || {
        }));
    }

    function loadSettings() {
        if (SettingsManager)
            draftSettings = cloneSettings(SettingsManager.settings);
        else
            draftSettings = ({
        });
    }

    function applySettings() {
        if (!SettingsManager)
            return ;

        SettingsManager.settings = cloneSettings(draftSettings);
        SettingsManager.save();
    }

    function valueOr(key, fallbackValue) {
        return draftSettings[key] !== undefined ? draftSettings[key] : fallbackValue;
    }

    function setValue(key, value) {
        var next = cloneSettings(draftSettings);
        next[key] = value;
        draftSettings = next;
    }

    function indexOfValue(values, targetValue, fallbackIndex) {
        for (var i = 0; i < values.length; ++i) {
            if (values[i] === targetValue)
                return i;

        }
        return fallbackIndex;
    }

    function pluginPathsText(formatName) {
        var values = draftSettings["pluginPaths" + formatName];
        if (!values)
            return "";

        return values.join("\n");
    }

    function setPluginEnabled(formatName, enabled) {
        setValue("pluginEnable" + formatName, enabled);
    }

    function setPluginPathsFromText(formatName, textValue) {
        var lines = textValue.split(/\r?\n/);
        var paths = [];
        for (var i = 0; i < lines.length; ++i) {
            var path = lines[i].trim();
            if (path.length > 0)
                paths.push(path);

        }
        setValue("pluginPaths" + formatName, paths);
    }

    width: 760
    height: 680
    title: qsTr("環境設定")
    Component.onCompleted: loadSettings()
    onVisibleChanged: {
        if (visible)
            loadSettings();

    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 10

        TabBar {
            id: tabBar

            Layout.fillWidth: true

            TabButton {
                text: qsTr("一般")
            }

            TabButton {
                text: qsTr("性能")
            }

            TabButton {
                text: qsTr("タイムライン")
            }

            TabButton {
                text: qsTr("外観")
            }

            TabButton {
                text: qsTr("新規プロジェクト")
            }

            TabButton {
                text: qsTr("書き出し")
            }

            TabButton {
                text: qsTr("デコードと音声")
            }

            TabButton {
                text: qsTr("プラグイン")
            }

            TabButton {
                text: qsTr("ショートカット")
            }

        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabBar.currentIndex

            GeneralSettingsPage {
                draftSettings: root.draftSettings
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            PerformanceSettingsPage {
                draftSettings: root.draftSettings
                renderThreadValues: root.renderThreadValues
                renderThreadLabels: root.renderThreadLabels
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            TimelineSettingsPage {
                draftSettings: root.draftSettings
                timeUnitValues: root.timeUnitValues
                timeUnitLabels: root.timeUnitLabels
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            AppearanceSettingsPage {
                draftSettings: root.draftSettings
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            ProjectSettingsPage {
                draftSettings: root.draftSettings
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            ExportSettingsPage {
                draftSettings: root.draftSettings
                videoCodecValues: root.videoCodecValues
                videoCodecLabels: root.videoCodecLabels
                audioCodecValues: root.audioCodecValues
                audioCodecLabels: root.audioCodecLabels
                audioChannelValues: root.audioChannelValues
                audioChannelLabels: root.audioChannelLabels
                blockSizeValues: root.blockSizeValues
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            DecodeAudioSettingsPage {
                draftSettings: root.draftSettings
                audioChannelValues: root.audioChannelValues
                audioChannelLabels: root.audioChannelLabels
                blockSizeValues: root.blockSizeValues
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
            }

            PluginSettingsPage {
                draftSettings: root.draftSettings
                pluginFormats: root.pluginFormats
                onValueChanged: (key, value) => {
                    return root.setValue(key, value);
                }
                onPluginEnabledChanged: (fmt, en) => {
                    return root.setPluginEnabled(fmt, en);
                }
                onPluginPathsChanged: (fmt, txt) => {
                    return root.setPluginPathsFromText(fmt, txt);
                }
            }

            ShortcutSettingsPage {
                draftSettings: root.draftSettings
                shortcutList: root.shortcutList
                onShortcutValueChanged: (actionId, value) => {
                    return root.setShortcutValue(actionId, value);
                }
            }

        }

        Frame {
            Layout.fillWidth: true
            padding: 8

            RowLayout {
                anchors.fill: parent
                spacing: 10

                Label {
                    Layout.fillWidth: true
                    text: qsTr("設定は「適用」または「OK」で保存されます")
                    color: root.secondaryTextColor
                    elide: Text.ElideRight
                }

                Button {
                    text: qsTr("再読込")
                    onClicked: root.loadSettings()
                }

                Button {
                    text: qsTr("適用")
                    onClicked: root.applySettings()
                }

                Button {
                    text: qsTr("OK")
                    highlighted: true
                    onClicked: {
                        root.applySettings();
                        root.hide();
                    }
                }

                Button {
                    text: qsTr("閉じる")
                    onClicked: root.hide()
                }

            }

        }

    }

}
