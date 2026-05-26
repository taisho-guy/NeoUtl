import QtQuick
import QtQuick.Controls
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseComputeEffect {
    // デフォルト値など

    id: root

    source: {
        var p = parent;
        while (p) {
            // fbCaptureItem を持ち、かつそれがエフェクト自身ではなく、テクスチャソースとしての性質を持つか確認
            if (p.fbCaptureItem !== undefined && p.fbCaptureItem !== null && p.fbCaptureItem !== root && p.fbCaptureItem.hasOwnProperty("recursive"))
                return p.fbCaptureItem;

            p = p.parent;
        }
        return null;
    }
    // Qt.resolvedUrl を使うことで、この QML ファイルと同じディレクトリにある QSB を絶対パスで指定できます
    computeShader: Qt.resolvedUrl("pixelsorter.comp.qsb")
    uniformMapping: ({
        "mix": "mixAmount"
    })

    // デバッグ用: シェーダーエラーの表示
    Label {
        anchors.centerIn: parent
        text: "Compute Error:\n" + root.computeError
        color: "red"
        font.bold: true
        visible: root.computeError !== undefined && root.computeError !== ""
        horizontalAlignment: Text.AlignHCenter

        background: Rectangle {
            color: "black"
            opacity: 0.7
            radius: 4
        }

    }

}
