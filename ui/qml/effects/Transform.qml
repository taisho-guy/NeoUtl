import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseEffect {
    id: root

    // 🚀 3D座標・マテリアル用 (BaseObjectが引き続きバインドして利用)
    readonly property vector3d outputPosition: {
        const x = evalNumber("x", 0);
        const y = evalNumber("y", 0);
        const z = evalNumber("z", 0);
        return Qt.vector3d(x, -y, z);
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
        return Qt.vector3d(cx, -cy, cz);
    }
    readonly property real outputOpacity: evalNumber("opacity", 1)
    readonly property int outputCullMode: {
        return evalParam("backfaceVisible", true) ? DefaultMaterial.NoCulling : DefaultMaterial.BackFaceCulling;
    }
    // 🚀 2Dキャプチャ/レンダリング用アフィンパラメータ
    readonly property real output2dScale: Math.max(0, evalNumber("scale", 100)) / 100
    readonly property real output2dX: evalNumber("x", 0)
    readonly property real output2dY: evalNumber("y", 0)
    readonly property real output2dRotationZ: evalNumber("rotationZ", 0)
    readonly property real outputCx: evalNumber("cx", 0)
    readonly property real outputCy: evalNumber("cy", 0)
    // 🚀 合成（ブレンド）モードの数値化
    readonly property int blendModeInt: {
        const m = evalParam("blendMode", "通常");
        if (m === "スクリーン")
            return 1;

        if (m === "乗算")
            return 2;

        if (m === "オーバーレイ")
            return 3;

        if (m === "焼き込み")
            return 4;

        if (m === "覆い焼き")
            return 5;

        return 0; // 通常
    }

    // 🚀 2Dテクスチャエフェクトとしてのシェーダー適用
    ShaderEffect {
        property variant source: root.sourceProxy
        // アフィン変換 uniform バインド
        property real translationX: root.output2dX
        property real translationY: root.output2dY
        property real scale: root.output2dScale
        property real rotationZ: root.output2dRotationZ * Math.PI / 180 // ラジアン変換
        property real cx: root.outputCx
        property real cy: root.outputCy
        property real opacityValue: root.outputOpacity
        property real targetWidth: root.width
        property real targetHeight: root.height
        property int blendMode: root.blendModeInt

        anchors.fill: parent
        fragmentShader: "transform.frag.qsb"
    }

}
