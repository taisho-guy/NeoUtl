import Qt5Compat.GraphicalEffects
import QtQuick
import QtQuick.Effects
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

Common.BaseObject {
    id: root

    property string textContent: evalString("text", "text", "テキスト")
    property string fontFamily: evalString("text", "fontFamily", "sans-serif")
    property real fontSize: evalNumber("text", "fontSize", 48)
    property bool fontBold: evalBool("text", "fontBold", false)
    property bool fontItalic: evalBool("text", "fontItalic", false)
    property real letterSpacing: evalNumber("text", "letterSpacing", 0)
    property real lineSpacing: evalNumber("text", "lineSpacing", 0)
    property int alignment: Math.round(evalNumber("text", "alignment", Text.AlignHCenter))
    property color textColor: evalColor("text", "color", "#ffffff")
    property bool outlineEnabled: evalBool("text", "outlineEnabled", false)
    property color outlineColor: evalColor("text", "outlineColor", "#000000")
    property real outlineWidth: evalNumber("text", "outlineWidth", 2)
    property bool shadowEnabled: evalBool("text", "shadowEnabled", false)
    property color shadowColor: evalColor("text", "shadowColor", "#80000000")
    property real shadowOffsetX: evalNumber("text", "shadowOffsetX", 5)
    property real shadowOffsetY: evalNumber("text", "shadowOffsetY", 5)
    property bool bgEnabled: evalBool("text", "bgEnabled", false)
    property color bgColor: evalColor("text", "backgroundColor", "#80000000")
    property real bgRadius: evalNumber("text", "backgroundRadius", 10)
    property real bgPaddingX: evalNumber("text", "backgroundPaddingX", 20)
    property real bgPaddingY: evalNumber("text", "backgroundPaddingY", 10)
    // 縁取りがはみ出さないようにパディングを確保
    readonly property real _pad: outlineEnabled ? Math.ceil(outlineWidth) + 2 : 2

    Model {
        source: "#Rectangle"
        visible: root.outputModelVisible
        scale: Qt.vector3d((root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.width : 1) / 100, (root.displayOutput && root.displayOutput.sourceItem ? root.displayOutput.sourceItem.height : 1) / 100, 1)

        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            blendMode: root.blendMode
            cullMode: root.cullMode

            diffuseMap: Texture {
                sourceItem: root.displayOutput
            }

        }

    }

    sourceItem: Item {
        // _pad 分 + 背景パディング分だけ拡大
        width: Math.max(textItem.implicitWidth + root._pad * 2 + (root.bgEnabled ? root.bgPaddingX * 2 : 0), 1)
        height: Math.max(textItem.implicitHeight + root._pad * 2 + (root.bgEnabled ? root.bgPaddingY * 2 : 0), 1)
        // opacity: 0 で不可視にしつつ SceneGraph には残す（BaseObject.qml の設計方針に準拠）
        visible: true
        opacity: 0

        Rectangle {
            anchors.fill: parent
            visible: root.bgEnabled
            color: root.bgColor
            radius: root.bgRadius
        }

        // テキスト＋縁取りをまとめるコンテナ
        // テキスト本体と「クールな」GPU 縁取り
        Item {
            id: textWrapper

            anchors.centerIn: parent
            width: textItem.implicitWidth + root._pad * 2
            height: textItem.implicitHeight + root._pad * 2

            Text {
                id: textItem

                anchors.centerIn: parent
                text: root.textContent
                font.family: root.fontFamily
                font.pixelSize: root.fontSize
                font.bold: root.fontBold
                font.italic: root.fontItalic
                font.weight: root.fontBold ? Font.Bold : Font.Normal
                font.letterSpacing: root.letterSpacing
                lineHeight: root.lineSpacing > 0 ? root.lineSpacing : 1
                lineHeightMode: root.lineSpacing > 0 ? Text.FixedHeight : Text.ProportionalHeight
                horizontalAlignment: root.alignment
                verticalAlignment: Text.AlignVCenter
                color: root.textColor
                renderType: Text.CurveRendering
            }

            Glow {
                anchors.fill: textItem
                source: textItem
                visible: root.outlineEnabled && root.outlineWidth > 0
                color: root.outlineColor
                radius: Math.ceil(root.outlineWidth)
                samples: Math.min(64, 1 + Math.ceil(root.outlineWidth) * 2)
                spread: 1
                transparentBorder: true
                // Textの上に被らないように z を下げる
                z: -1
            }

        }

        ShaderEffectSource {
            id: textCapture

            sourceItem: textWrapper
            hideSource: root.shadowEnabled
            live: true
            visible: false
            width: textWrapper.width
            height: textWrapper.height
        }

        // 影エフェクト: textCapture (FBOキャプチャ済み) をソースに使う
        // これにより textWrapper が非表示でも正しく影付きテキストが描画される
        MultiEffect {
            x: textWrapper.x
            y: textWrapper.y
            width: textWrapper.width
            height: textWrapper.height
            source: textCapture
            visible: root.shadowEnabled
            shadowEnabled: true
            shadowColor: root.shadowColor
            shadowBlur: 0
            shadowHorizontalOffset: root.shadowOffsetX
            shadowVerticalOffset: root.shadowOffsetY
        }

    }

}
