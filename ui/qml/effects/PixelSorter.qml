import QtQuick
import QtQuick.Controls
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseComputeEffect {
    // デフォルト値など

    id: root

    // 親スコープ (BaseObject) の fbCaptureItem を直接参照（Loader 階層を貫通させる）
    source: typeof fbCaptureItem !== "undefined" ? fbCaptureItem : null
    // Qt.resolvedUrl を使うことで、この QML ファイルと同じディレクトリにある QSB を絶対パスで指定できます
    computeShader: Qt.resolvedUrl("pixelsorter.comp.qsb")
    // C++ の params プロパティに直接マップする（BaseComputeEffect の実装に依存）
    params: ({
        "mix": 1
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
