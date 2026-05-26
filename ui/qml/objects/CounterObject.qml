import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property string mode: evalString("counter", "mode", "frame")
    property real startValue: evalNumber("counter", "startValue", 0)
    property real endValue: evalNumber("counter", "endValue", 100)
    property int digits: Math.max(0, Math.round(evalNumber("counter", "digits", 0)))
    property int decimals: Math.max(0, Math.round(evalNumber("counter", "decimals", 0)))
    property string prefix: evalString("counter", "prefix", "")
    property string suffix: evalString("counter", "suffix", "")
    property string fontFamily: evalString("counter", "fontFamily", "sans-serif")
    property real fontSize: evalNumber("counter", "fontSize", 64)
    property color textColor: evalColor("counter", "color", "#ffffff")
    property color outlineColor: evalColor("counter", "outlineColor", "#000000")
    property real outlineWidth: evalNumber("counter", "outlineWidth", 2)
    property real opacity: evalNumber("counter", "opacity", 1)

    readonly property real progress: clipDurationFrames > 0 ? Math.max(0, Math.min(1, relFrame / clipDurationFrames)) : 0
    readonly property real counterValue: {
        if (mode === "time")
            return relFrame / Math.max(projectFps, 0.001);
        if (mode === "value")
            return startValue + (endValue - startValue) * progress;
        return relFrame;
    }
    readonly property string numberText: {
        var text = Number(counterValue).toFixed(decimals);
        if (digits > 0) {
            var sign = text.charAt(0) === "-" ? "-" : "";
            var body = sign ? text.slice(1) : text;
            var parts = body.split(".");
            while (parts[0].length < digits)
                parts[0] = "0" + parts[0];
            text = sign + parts.join(".");
        }
        return prefix + text + suffix;
    }
    readonly property real pad: Math.max(4, outlineWidth * 3 + 4)

    sourceItem: sourceItem

    Item {
        id: sourceItem

        visible: false
        width: Math.max(1, counterText.implicitWidth + root.pad * 2)
        height: Math.max(1, counterText.implicitHeight + root.pad * 2)

        Text {
            id: counterText

            anchors.centerIn: parent
            text: root.numberText
            color: root.textColor
            font.family: root.fontFamily
            font.pixelSize: root.fontSize
            style: root.outlineWidth > 0 ? Text.Outline : Text.Normal
            styleColor: root.outlineColor
        }
    }

    Model {
        source: "#Rectangle"
        visible: root.outputModelVisible
        scale: Qt.vector3d((root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.width : sourceItem.width) / 100, (root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.height : sourceItem.height) / 100, 1)
        opacity: root.opacity

        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            blendMode: root.blendMode
            cullMode: root.cullMode

            diffuseMap: Texture {
                sourceItem: root.displayOutput
            }
        }
    }
}
