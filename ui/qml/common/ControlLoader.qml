import "." as Common
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

Loader {
    // 数値
    // 整数

    id: controlLoader

    property var definition: ({
    })
    property var value: null
    property var effectRootRef: null
    // 親(SettingDialog)から補間状態を受け取るためのプロパティ
    property int startFrameState: 0
    property int endFrameState: 0
    property bool rightInteractiveState: false

    signal valueModified(var newValue)
    signal paramButtonClicked()
    signal startValueModified(var val)
    signal endValueModified(var val)

    function _label() {
        var d = controlLoader.definition;
        if (!d)
            return qsTr("パラメータ");

        return (d.label && d.label !== "") ? d.label : (d.param || d.name || qsTr("パラメータ"));
    }

    function _resolveSwatchColor(val) {
        if (!val || typeof val !== "string")
            return "#ffffff";

        var v = val.trim();
        if (!v.startsWith("{"))
            return v;

        try {
            var obj = JSON.parse(v);
            if (obj.stops && obj.stops.length > 0)
                return obj.stops[0].color;

        } catch (e) {
        }
        return "#ffffff";
    }

    function _optionValue(option) {
        if (option && typeof option === "object")
            return option.value !== undefined ? option.value : (option.id !== undefined ? option.id : option.label);

        return option;
    }

    function _optionText(option) {
        if (option && typeof option === "object")
            return option.label !== undefined ? option.label : String(_optionValue(option));

        return String(option);
    }

    function _optionIndex(options, value) {
        if (!options)
            return -1;

        for (var i = 0; i < options.length; i++) {
            if (_optionValue(options[i]) === value)
                return i;

        }
        return -1;
    }

    function _optionModel(options) {
        if (!options)
            return [];

        var result = [];
        for (var i = 0; i < options.length; i++) {
            result.push({
                "value": _optionValue(options[i]),
                "label": _optionText(options[i])
            });
        }
        return result;
    }

    // キーフレーム区間探索（共通ロジック）
    function _findKeyframeInterval(kfs, cur, total) {
        var s = 0, e = total;
        if (!kfs || kfs.length === 0)
            return {
            "start": s,
            "end": e
        };

        var found = false;
        for (var i = kfs.length - 1; i >= 0; i--) {
            if (kfs[i].frame <= cur) {
                s = kfs[i].frame;
                e = (i + 1 < kfs.length) ? kfs[i + 1].frame : total;
                found = true;
                break;
            }
        }
        if (!found)
            e = kfs[0].frame;

        return {
            "start": s,
            "end": e
        };
    }

    // === コンポーネント選択 ===
    sourceComponent: {
        if (!definition)
            return unknownComponent;

        switch (definition.type) {
        case "float":
        case "number":
        case "slider":
        case "spinner":
            return floatComponent;
        case "int":
        case "integer":
        case "scene_id":
            return intComponent;
        case "color":
            // 色
            return colorComponent;
        case "bool":
        case "boolean":
            return boolComponent;
        case "path":
        case "file":
            // パス・ファイル
            return pathComponent;
        case "string":
        case "text":
            // 文字列
            return stringComponent;
        case "enum":
            return enumComponent;
        case "combo":
            return comboComponent;
        case "font":
            return fontComponent;
        case "header":
            return headerComponent;
        default:
            return unknownComponent;
        }
    }

    SystemPalette {
        id: sysPalette

        colorGroup: SystemPalette.Active
    }

    Component {
        id: floatComponent

        Common.ParamControl {
            paramName: controlLoader._label()
            startValue: (controlLoader.value !== undefined && controlLoader.value !== null) ? Number(controlLoader.value) : (controlLoader.definition.default || 0)
            endValue: startValue
            isRangeMode: false
            minValue: controlLoader.definition.min !== undefined ? controlLoader.definition.min : -100000
            maxValue: controlLoader.definition.max !== undefined ? controlLoader.definition.max : 100000
            decimals: controlLoader.definition.decimals !== undefined ? controlLoader.definition.decimals : 2
            onStartValueModified: function(val) {
                controlLoader.valueModified(val);
            }
            onEndValueModified: function(val) {
                controlLoader.valueModified(val);
            }
            onParamButtonClicked: {
                controlLoader.paramButtonClicked();
            }
        }

    }

    Component {
        id: intComponent

        RowLayout {
            spacing: 8

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
                elide: Text.ElideRight
            }

            TextField {
                text: String(controlLoader.value !== undefined ? controlLoader.value : (controlLoader.definition.default || 0))
                Layout.preferredWidth: 80
                horizontalAlignment: TextInput.AlignRight
                selectByMouse: true
                onEditingFinished: {
                    var val = parseInt(text);
                    if (!isNaN(val))
                        controlLoader.valueModified(val);

                }

                validator: IntValidator {
                    bottom: controlLoader.definition.min !== undefined ? controlLoader.definition.min : -2.14748e+09
                    top: controlLoader.definition.max !== undefined ? controlLoader.definition.max : 2.14748e+09
                }

            }

            Label {
                text: controlLoader.definition.unit || ""
                color: Qt.rgba(sysPalette.text.r, sysPalette.text.g, sysPalette.text.b, 0.6)
                visible: text.length > 0
            }

            Item {
                Layout.fillWidth: true
            }

        }

    }

    Component {
        id: colorComponent

        RowLayout {
            id: colorRow

            property var _em: controlLoader.effectRootRef ? controlLoader.effectRootRef.effectModel : null
            property int _effIdx: controlLoader.effectRootRef ? controlLoader.effectRootRef.effectIndex : 0
            property string _key: controlLoader.definition.param || controlLoader.definition.name || ""
            property int _clipDur: Workspace.currentTimeline ? Workspace.currentTimeline.clipDurationFrames : 100
            property real _fps: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60
            property int _curFrame: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Workspace.currentTimeline.transport.currentFrame - Workspace.currentTimeline.clipStartFrame : 0
            property int _rev: 0
            property int _startFrame: controlLoader.startFrameState
            property int _endFrame: controlLoader.endFrameState
            property bool _rightInteractive: controlLoader.rightInteractiveState
            property bool _hasKf: true
            property var _startVal: {
                var _ = colorRow._rev;
                return (_em) ? (_em.evaluatedParam(_key, _startFrame, _fps) || controlLoader.value || "#ffffff") : (controlLoader.value || "#ffffff");
            }
            property var _endVal: {
                var _ = colorRow._rev;
                return (_em) ? (_em.evaluatedParam(_key, _endFrame, _fps) || controlLoader.value || "#ffffff") : (controlLoader.value || "#ffffff");
            }

            function _commit(frame, val) {
                if (frame === _startFrame)
                    controlLoader.startValueModified(val);
                else if (frame === _endFrame)
                    controlLoader.endValueModified(val);
                else
                    controlLoader.valueModified(val);
            }

            spacing: 8

            Connections {
                function onKeyframeTracksChanged() {
                    colorRow._rev++;
                }

                function onParamsChanged() {
                    colorRow._rev++;
                }

                target: colorRow._em
            }

            ColorDialog {
                id: startColorDlg

                onAccepted: colorRow._commit(colorRow._startFrame, selectedColor.toString())
            }

            ColorDialog {
                id: endColorDlg

                onAccepted: colorRow._commit(colorRow._endFrame, selectedColor.toString())
            }

            // fillWidth:true + preferredWidth:120 で leftSlider と同幅
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredWidth: 120
                Layout.minimumWidth: 40
                height: 24
                radius: 3
                color: controlLoader._resolveSwatchColor(colorRow._startVal)
                border.color: startSwatchMa.containsMouse ? sysPalette.highlight : sysPalette.mid
                border.width: 1
                ToolTip.visible: startSwatchMa.containsMouse
                ToolTip.text: colorRow._hasKf ? qsTr("開始色 (f%1)").arg(colorRow._startFrame) : qsTr("色を選択")

                MouseArea {
                    id: startSwatchMa

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        var c = colorRow._startVal || "#ffffff";
                        // HexArgb (#aarrggbb) を Qt color 型に安全変換
                        startColorDlg.selectedColor = Qt.color(c);
                        startColorDlg.open();
                    }
                }

            }

            TextField {
                id: startHexField

                Layout.preferredWidth: 70
                Layout.minimumWidth: 50
                text: colorRow._startVal
                horizontalAlignment: TextInput.AlignHCenter
                selectByMouse: true
                onEditingFinished: {
                    colorRow._commit(colorRow._startFrame, text);
                }

                Binding on text {
                    when: !startHexField.activeFocus
                    value: colorRow._startVal
                }

            }

            Button {
                id: paramBtn

                Layout.preferredWidth: 100
                text: controlLoader._label()
                onClicked: controlLoader.paramButtonClicked()
            }

            TextField {
                id: endHexField

                Layout.preferredWidth: 70
                Layout.minimumWidth: 50
                text: colorRow._endVal
                enabled: colorRow._rightInteractive
                opacity: colorRow._rightInteractive ? 1 : 0.45
                horizontalAlignment: TextInput.AlignHCenter
                selectByMouse: true
                onEditingFinished: {
                    colorRow._commit(colorRow._endFrame, text);
                }

                Binding on text {
                    when: !endHexField.activeFocus
                    value: colorRow._endVal
                }

            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredWidth: 120
                Layout.minimumWidth: 40
                height: 24
                radius: 3
                color: controlLoader._resolveSwatchColor(colorRow._endVal)
                opacity: colorRow._rightInteractive ? 1 : 0.45
                border.color: colorRow._rightInteractive ? (endSwatchMa.containsMouse ? sysPalette.highlight : sysPalette.mid) : sysPalette.dark
                border.width: 1
                ToolTip.visible: colorRow._rightInteractive && endSwatchMa.containsMouse
                ToolTip.text: qsTr("終了色 (f%1)").arg(colorRow._endFrame)

                MouseArea {
                    id: endSwatchMa

                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    enabled: colorRow._rightInteractive
                    onClicked: {
                        var c = colorRow._endVal || "#ffffff";
                        // HexArgb (#aarrggbb) を Qt color 型に安全変換
                        endColorDlg.selectedColor = Qt.color(c);
                        endColorDlg.open();
                    }
                }

            }

        }

    }

    Component {
        id: boolComponent

        CheckBox {
            text: controlLoader._label()
            checked: controlLoader.value === true
            Layout.fillWidth: true
            onToggled: {
                controlLoader.valueModified(checked);
            }
        }

    }

    Component {
        id: pathComponent

        RowLayout {
            spacing: 8

            FileDialog {
                id: fileDialog

                title: controlLoader.definition.label || qsTr("ファイルを選択")
                currentFile: {
                    var v = controlLoader.value;
                    if (!v || v.toString() === "")
                        return "";

                    var s = v.toString();
                    return s.startsWith("file://") ? s : "file://" + s;
                }
                nameFilters: {
                    var f = controlLoader.definition.filter;
                    if (Array.isArray(f))
                        return f;

                    return f ? [f, "All files (*)"] : ["All files (*)"];
                }
                onAccepted: {
                    var path = selectedFile.toString();
                    if (Qt.platform.os === "windows")
                        path = path.replace(/^(file:\/{3})/, "");
                    else
                        path = path.replace(/^(file:\/\/)/, "");
                    controlLoader.valueModified(path);
                }
            }

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
            }

            TextField {
                text: String(controlLoader.value || "")
                Layout.fillWidth: true
                selectByMouse: true
                onEditingFinished: {
                    controlLoader.valueModified(text);
                }
            }

            Button {
                text: "..."
                Layout.preferredWidth: 30
                onClicked: {
                    fileDialog.open();
                }
            }

        }

    }

    Component {
        id: stringComponent

        RowLayout {
            spacing: 8
            Layout.alignment: Qt.AlignTop

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
                Layout.alignment: Qt.AlignTop
                Layout.topMargin: 8
            }

            ScrollView {
                Layout.fillWidth: true
                Layout.maximumHeight: 400
                Layout.preferredHeight: Math.max(40, textAreaItem.implicitHeight)
                clip: true

                TextArea {
                    id: textAreaItem

                    text: String(controlLoader.value !== undefined && controlLoader.value !== null ? controlLoader.value : "")
                    selectByMouse: true
                    wrapMode: TextArea.Wrap
                    onTextChanged: {
                        if (activeFocus)
                            controlLoader.valueModified(text);

                    }
                    onEditingFinished: {
                        controlLoader.valueModified(text);
                    }

                    Binding on text {
                        when: !textAreaItem.activeFocus
                        value: String(controlLoader.value !== undefined && controlLoader.value !== null ? controlLoader.value : "")
                    }

                }

            }

        }

    }

    Component {
        id: enumComponent

        RowLayout {
            spacing: 8

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
            }

            ComboBox {
                id: enumCombo

                model: controlLoader._optionModel(controlLoader.definition.options)
                Layout.fillWidth: true
                textRole: "label"
                currentIndex: {
                    var idx = controlLoader._optionIndex(model, controlLoader.value);
                    return idx >= 0 ? idx : 0;
                }
                onActivated: {
                    controlLoader.valueModified(controlLoader._optionValue(model[index]));
                }
            }

        }

    }

    Component {
        id: headerComponent

        ColumnLayout {
            Layout.fillWidth: true
            Layout.topMargin: 12
            Layout.bottomMargin: 4

            Label {
                text: controlLoader.definition.label || qsTr("設定")
                font.bold: true
                font.pixelSize: 13
                color: sysPalette.text
            }

            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: sysPalette.mid
            }

        }

    }

    Component {
        id: comboComponent

        RowLayout {
            spacing: 8

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
            }

            ComboBox {
                id: combo

                property var sourceObj: {
                    if (controlLoader.definition.source === "Workspace.currentTimeline")
                        return Workspace.currentTimeline;

                    return null;
                }

                Layout.fillWidth: true
                model: sourceObj ? sourceObj[controlLoader.definition.sourceProperty] : []
                textRole: controlLoader.definition.textRole || "name"
                valueRole: controlLoader.definition.valueRole || "id"
                currentIndex: {
                    var val = controlLoader.value;
                    if (!model)
                        return -1;

                    for (var i = 0; i < count; i++) {
                        var item = model[i];
                        if (item && item[valueRole] == val)
                            return i;

                    }
                    return -1;
                }
                onActivated: function(index) {
                    if (index >= 0 && model && model[index])
                        controlLoader.valueModified(model[index][valueRole]);

                }
            }

        }

    }

    Component {
        id: fontComponent

        RowLayout {
            Layout.fillWidth: true
            spacing: 6

            FontDialog {
                id: fontPickerDialog

                title: controlLoader.definition.label || qsTr("フォントを選択")
                currentFont.family: controlLoader.value || ""
                onAccepted: {
                    controlLoader.valueModified(selectedFont.family);
                }
            }

            Label {
                text: controlLoader._label()
                color: sysPalette.text
                Layout.preferredWidth: 80
                elide: Text.ElideRight
            }

            Button {
                Layout.fillWidth: true
                text: controlLoader.value ? controlLoader.value : qsTr("デフォルト")
                font.family: controlLoader.value || ""
                font.pixelSize: 13
                onClicked: {
                    fontPickerDialog.open();
                }
            }

        }

    }

    Component {
        id: unknownComponent

        Text {
            text: "Unknown Type: " + (controlLoader.definition ? controlLoader.definition.type : "null")
            color: sysPalette.highlight
            font.pixelSize: 10
        }

    }

}
