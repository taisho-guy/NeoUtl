import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseEffect {
    id: root

    readonly property vector3d outputPosition: {
        const x = evalNumber("x", 0);
        const y = evalNumber("y", 0);
        const z = evalNumber("z", 0);
        return Qt.vector3d(x, y, z);
    }
    readonly property vector3d outputRotation: {
        const rx = evalNumber("rotationX", 0);
        const ry = evalNumber("rotationY", 0);
        const rz = evalNumber("rotationZ", 0);
        return Qt.vector3d(rx, ry, -rz);
    }
    readonly property vector3d outputPivot: {
        const cx = evalNumber("cx", 0);
        const cy = evalNumber("cy", 0);
        const cz = evalNumber("cz", 0);
        return Qt.vector3d(cx, cy, cz);
    }
    readonly property real outputOpacity: evalNumber("opacity", 1)
    readonly property int outputCullMode: {
        return evalParam("backfaceVisible", true) ? DefaultMaterial.NoCulling : DefaultMaterial.BackFaceCulling;
    }
    readonly property real output2dScale: Math.max(0, evalNumber("scale", 100)) / 100
    readonly property real output2dX: evalNumber("x", 0)
    readonly property real output2dY: evalNumber("y", 0)
    readonly property real output2dRotationZ: evalNumber("rotationZ", 0)
    readonly property real outputCx: evalNumber("cx", 0)
    readonly property real outputCy: evalNumber("cy", 0)
    readonly property int blendModeInt: {
        const m = evalParam("blendMode", "通常");
        if (m === "スクリーン")
            return 1;

        if (m === "乗算")
            return 2;

        if (m === "オーバーレイ")
            return 3;

        if (m === "加算")
            return 4;

        if (m === "減算")
            return 5;

        if (m === "比較（明）")
            return 6;

        if (m === "比較（暗）")
            return 7;

        if (m === "色反転")
            return 8;

        if (m === "ソフトライト")
            return 9;

        if (m === "ハードライト")
            return 10;

        if (m === "差の絶対値")
            return 11;

        if (m === "色相")
            return 12;

        if (m === "彩度")
            return 13;

        if (m === "カラー")
            return 14;

        if (m === "輝度")
            return 15;

        return 0; // 通常
    }

    ShaderEffect {
        property variant source: root.sourceProxy
        // アフィン変換 uniform バインド
        property real translationX: 0
        property real translationY: 0
        property real scale: 1
        property real rotationZ: 0
        property real cx: 0
        property real cy: 0
        property real opacityValue: root.outputOpacity
        property real targetWidth: root.width
        property real targetHeight: root.height
        property int blendMode: root.blendModeInt

        anchors.fill: parent
        fragmentShader: "transform.frag.qsb"
    }

}
