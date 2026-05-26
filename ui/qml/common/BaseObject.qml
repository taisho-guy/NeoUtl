import "." as Common
import QtQuick
import QtQuick3D

Node {
    id: base

    // CompositeView 側から渡される「Window配下のItem」。ここに2D系を寄せる
    property Item renderHost: null
    property var owned2D: []
    // CompositeView から自動注入されるプロパティ
    property int clipId: -1
    property int sceneId: -1
    property int clipStartFrame: 0
    property int clipDurationFrames: 0
    // NodeLoader.onItemChanged で注入されるレイヤー番号
    property int clipLayer: -1
    // CompositeView で計算済みの実効2D変換。フレームバッファ用の
    // キャプチャは View3D の親Node変換を受けないため、ここで再現する。
    property real clipNodeScaleX: 1
    property real clipNodeScaleY: 1
    property real clipNodePosX: 0
    property real clipNodePosY: 0
    property real clipNodeRotZ: 0
    property real clipNodeOpacity: 1
    property bool outputModelVisible: true
    property alias fbCaptureItem: _fbCaptureItemImpl
    // transformエフェクトのモデルを探す
    readonly property var transformModel: {
        for (var i = 0; i < rawEffectModels.length; i++) {
            if (rawEffectModels[i].id === "transform")
                return rawEffectModels[i];

        }
        return null;
    }
    // transformLoader.item (Transform.qmlのインスタンス) が存在するか
    readonly property bool hasTransform: transformLoader.status === Loader.Ready && transformLoader.item
    property alias revision: base._tmRev
    property int _tmRev: 0
    readonly property int blendMode: {
        var m = evalString("transform", "blendMode", qsTr("通常"));
        if (m === qsTr("スクリーン"))
            return DefaultMaterial.Screen;

        if (m === qsTr("乗算"))
            return DefaultMaterial.Multiply;

        if (m === qsTr("オーバーレイ"))
            return DefaultMaterial.Overlay;

        if (m === qsTr("焼き込み"))
            return DefaultMaterial.ColorBurn;

        if (m === qsTr("覆い焼き"))
            return DefaultMaterial.ColorDodge;

        return DefaultMaterial.SourceOver;
    }
    // カリングモード (Transform.qmlから取得)
    readonly property int cullMode: hasTransform ? transformLoader.item.outputCullMode : DefaultMaterial.NoCulling
    // 自動計算プロパティ
    property int currentFrame: 0
    // Will be overridden by CompositeView
    readonly property int relFrame: currentFrame - clipStartFrame
    readonly property real projectFps: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60
    property var rawEffectModels: []
    // フィルタ系エフェクト（transform/object以外）
    readonly property var filterModels: {
        var res = [];
        for (var i = 0; i < rawEffectModels.length; i++) {
            var eff = rawEffectModels[i];
            if (eff.kind === "effect")
                res.push(eff);

        }
        return res;
    }
    // 子クラスがオーバーライドするプロパティ
    property Item sourceItem
    property alias renderer: rendererInstance
    property Item displayOutput: rendererInstance.output

    function evalParam(effectId, paramName, fallback) {
        var _ = base._tmRev; // リアクティブ依存
        if (base.rawEffectModels) {
            for (var i = 0; i < base.rawEffectModels.length; i++) {
                if (base.rawEffectModels[i].id === effectId) {
                    if (base.rawEffectModels[i].evaluatedParam) {
                        var v = base.rawEffectModels[i].evaluatedParam(paramName, base.relFrame, base.projectFps);
                        if (v !== undefined && v !== null)
                            return v;

                    }
                    if (base.rawEffectModels[i].params && base.rawEffectModels[i].params[paramName] !== undefined)
                        return base.rawEffectModels[i].params[paramName];

                }
            }
        }
        return fallback;
    }

    function evalString(effectId, paramName, fallback) {
        var v = evalParam(effectId, paramName, undefined);
        return (v !== undefined && v !== null) ? String(v) : fallback;
    }

    function evalNumber(effectId, paramName, fallback) {
        var v = evalParam(effectId, paramName, undefined);
        return (v !== undefined && v !== null && v !== "") ? Number(v) : fallback;
    }

    function evalBool(effectId, paramName, fallback) {
        var v = evalParam(effectId, paramName, undefined);
        return (v !== undefined && v !== null) ? Boolean(v) : fallback;
    }

    function evalColor(effectId, paramName, fallback) {
        var v = evalParam(effectId, paramName, undefined);
        return (v !== undefined && v !== null) ? v : fallback;
    }

    function adopt2D(item) {
        if (!item || !renderHost)
            return ;

        if (item.parent === renderHost)
            return ;

        item.parent = renderHost;
        // visible を落とすと SceneGraph から外れてテクスチャ更新が止まり得るので触らない。
        // 表示は CompositeView 側の host opacity と ShaderEffectSource.hideSource に任せる。
        owned2D.push(item);
    }

    // NodeのプロパティをtransformModelにバインド
    position: hasTransform ? transformLoader.item.outputPosition : Qt.vector3d(0, 0, 0)
    eulerRotation: hasTransform ? transformLoader.item.outputRotation : Qt.vector3d(0, 0, 0)
    pivot: hasTransform ? transformLoader.item.outputPivot : Qt.vector3d(0, 0, 0)
    scale: Qt.vector3d(1, 1, 1) // 下のModelで個別に設定
    // renderHost が後からセットされても確実に移送する
    onRenderHostChanged: {
        adopt2D(sourceItem);
        adopt2D(rendererInstance);
        adopt2D(_fbCaptureItemImpl);
    }
    Component.onCompleted: {
        // 各オブジェクト(TextObject/RectObject)が set してくる sourceItem を移す
        adopt2D(base.sourceItem);
        // ObjectRenderer(= ShaderEffectSource/effectsチェーン)も移す
        adopt2D(rendererInstance);
        adopt2D(_fbCaptureItemImpl);
    }
    Component.onDestruction: {
        for (var i = 0; i < owned2D.length; i++) {
            try {
                if (owned2D[i])
                    owned2D[i].destroy();

            } catch (e) {
            }
        }
        owned2D = [];
    }
    // sourceItem は常に非表示（renderer.output のみ表示）
    onSourceItemChanged: {
        if (sourceItem) {
            // キャプチャ安定化のためvisibleは落とさず、不可視化はopacityで行う
            sourceItem.visible = true;
            sourceItem.opacity = 1;
        }
    }
    onRelFrameChanged: {
        if (hasTransform)
            transformLoader.item.frame = relFrame;

    }

    Instantiator {
        model: base.rawEffectModels

        Connections {
            function onParamsChanged() {
                base._tmRev++;
            }

            function onKeyframeTracksChanged() {
                base._tmRev++;
            }

            target: modelData
            ignoreUnknownSignals: true
        }

    }

    Loader {
        id: transformLoader

        source: (root.transformModel && root.transformModel.qmlSource) ? root.transformModel.qmlSource : ""
        // BaseEffectのプロパティを注入
        onLoaded: {
            item.source = null; // Transformはsourceを持たない
            item.params = root.transformModel.params;
            item.effectModel = root.transformModel;
            item.frame = root.relFrame;
        }
    }

    Item {
        id: _fbCaptureItemImpl

        readonly property int fbBlendMode: {
            const tModel = base.transformModel;
            if (!tModel)
                return 0;

            const m = tModel.params["blendMode"] || "通常";
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
        readonly property real fbOpacityValue: 1

        width: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.width : 1920
        height: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.height : 1080
        visible: true // SceneGraph に残すため true (opacity は renderHost 側で 0)

        Item {
            id: fbTransformItem

            // rendererInstanceのimplicitサイズを参照することで、プロジェクトサイズへの強制リサイズを防ぎます
            width: (rendererInstance && rendererInstance.output && rendererInstance.output.sourceItem ? (rendererInstance.output.sourceItem.implicitWidth || rendererInstance.output.sourceItem.width) : 1) * base.clipNodeScaleX
            height: (rendererInstance && rendererInstance.output && rendererInstance.output.sourceItem ? (rendererInstance.output.sourceItem.implicitHeight || rendererInstance.output.sourceItem.height) : 1) * base.clipNodeScaleY
            // AviUtl 座標系: 中心(0,0)、Y下プラス → Qt2D: 中心 = parent の center + offset
            x: _fbCaptureItemImpl.width / 2 + base.clipNodePosX - width / 2
            y: _fbCaptureItemImpl.height / 2 - base.clipNodePosY - height / 2
            rotation: -base.clipNodeRotZ
            opacity: base.clipNodeOpacity

            ShaderEffectSource {
                anchors.fill: parent
                sourceItem: renderer.finalItem
                live: true
                hideSource: true // finalItem (opacity:0 かもしれないが) を隠す
            }

        }

    }

    // レンダラー自動配置
    Common.ObjectRenderer {
        id: rendererInstance

        originalSource: base.sourceItem
        effectModels: base.filterModels
        relFrame: base.relFrame
    }

    sourceItem: Item {
        // デフォルトはダミー（visible: falseは子側で設定）
        width: 1
        height: 1
    }

}
