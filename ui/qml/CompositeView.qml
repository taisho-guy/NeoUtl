import QtQml
import QtQuick
import QtQuick3D
import "common" as Common
import "common/Logger.js" as Logger

Item {
    id: root

    property var layerStates: ({
    })
    // 外部から注入可能なプロパティ (デフォルトはTimelineBridgeから取得)
    property var clipModel: []
    property int projectWidth: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.width : 1920
    property int projectHeight: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.height : 1080
    property int currentFrame: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Workspace.currentTimeline.transport.currentFrame : 0
    property int sceneId: -1
    readonly property int hiddenZ: -9999
    // Component cache to prevent redundant Qt.createComponent calls
    property var _componentCache: ({
    })
    property bool exportMode: false
    property alias view3D: view
    // グループ制御管理
    property var groupControls: []
    // カメラ制御管理
    property var activeCameraControl: null

    // 各クリップのレンダラー出力が確定・変更されたことを通知する集中シグナル
    signal childRendererOutputsChanged()

    function getLayerVisible(layer) {
        if (!layerStates)
            return true;

        var state = layerStates[layer];
        return state !== undefined ? state.visible : true;
    }

    function registerCameraControl(cc) {
        activeCameraControl = cc;
    }

    function unregisterCameraControl(cc) {
        if (activeCameraControl === cc)
            activeCameraControl = null;

    }

    function getCachedComponent(url) {
        if (_componentCache[url])
            return _componentCache[url];

        var c = Qt.createComponent(url, Component.Asynchronous);
        _componentCache[url] = c;
        return c;
    }

    function registerGroupControl(gc) {
        for (var i = 0; i < groupControls.length; ++i) {
            if (groupControls[i] === gc)
                return ;

        }
        groupControls.push(gc);
        groupControlsChanged(); // 配列変更を通知して再計算を促す
    }

    function unregisterGroupControl(gc) {
        var idx = -1;
        for (var i = 0; i < groupControls.length; ++i) {
            if (groupControls[i] === gc) {
                idx = i;
                break;
            }
        }
        if (idx !== -1) {
            groupControls.splice(idx, 1);
            groupControlsChanged();
        }
    }

    // 2Dレンダー（sourceItem/effects/ShaderEffectSource）を必ずQQuickWindow配下に置くためのホスト
    // visible:false でもWindow配下に居ればSceneGraph/Timerが正常に動く
    Item {
        id: offscreenRenderHost

        anchors.fill: parent
        visible: true
        opacity: 0
        enabled: false
        z: hiddenZ
    }

    // 背景
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        z: -2
    }

    // プレビューコンテナ
    View3D {
        id: view

        // プロジェクト設定の解像度を取得
        property int projW: root.projectWidth
        property int projH: root.projectHeight
        // アスペクト比計算
        property double aspect: projW / projH
        // 現在のクリップ内での相対時間 (0.0 ~ 1.0)
        property double currentClipTimeRatio: (Workspace.currentTimeline) ? Math.max(0, Math.min(1, (root.currentFrame - Workspace.currentTimeline.clipStartFrame) / Workspace.currentTimeline.clipDurationFrames)) : 0

        camera: activeCameraControl ? activeCameraControl.camera : mainCamera
        // 親に収まる最大サイズを計算 (Letterboxing)
        width: root.exportMode ? projW : Math.min(parent.width, parent.height * aspect)
        height: root.exportMode ? projH : Math.min(parent.height, parent.width / aspect)
        anchors.fill: parent
        focus: true
        Keys.onSpacePressed: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.togglePlay();

        }

        // プロジェクト領域の背景
        Rectangle {
            anchors.fill: parent
            color: "#0a0a0a"
            z: -1
        }

        // カメラ設定
        PerspectiveCamera {
            id: mainCamera

            property real distance: view.projH / (2 * Math.tan(fieldOfView * Math.PI / 360))

            // 動的カメラ距離計算: projH/2 / tan(fieldOfView/2)
            fieldOfView: 30
            position: Qt.vector3d(0, 0, distance)
            clipFar: 5000
        }

        DirectionalLight {
            eulerRotation.x: -30
            z: 1000
        }

        // プロジェクトの枠線（解像度ガイド）
        Node {
            // 画面中央を (0,0) としたときの枠
            Model {
                source: "#Rectangle"
                scale: Qt.vector3d(root.projectWidth / 100, root.projectHeight / 100, 1)
                position: Qt.vector3d(0, 0, -10)
                visible: false // 邪魔なので一旦隠す

                materials: DefaultMaterial {
                    diffuseColor: "#22000000"
                }
                // 奥に配置

            }

        }

        // Instantiator の親となるノード
        Node {
            id: sceneRoot
        }

        // 動的オブジェクト生成用
        Instantiator {
            // 拡大率は親の影響を受けず自身のサイズとして適用（AviUtl仕様依存）
            // console.log("[Debug] CompositeView: Hiding clip", clipIdRole, "due to scene mismatch. root:", root.sceneId, "clip:", clipSceneIdRole)

            model: root.clipModel
            onObjectAdded: (index, object) => {
                object.parent = sceneRoot;
            }
            onObjectRemoved: (index, object) => {
                object.parent = null;
            }

            delegate: Node {
                id: clipNode

                // C++モデル(model)とJS配列(modelData)の両方に対応
                property var _clipData: (typeof modelData !== "undefined") ? modelData : model
                property int clipIdRole: _clipData.id
                property int clipSceneIdRole: _clipData.sceneId !== undefined ? _clipData.sceneId : -1
                property string clipTypeRole: _clipData.type
                property int clipLayerRole: _clipData.layer
                property int clipStartFrameRole: _clipData.startFrame
                property int clipDurationFramesRole: _clipData.durationFrames
                property url clipQmlSourceRole: _clipData.qmlSource || ""
                property var clipEffectModelsRole: _clipData.effectModels || []
                property Item fbRendererOutput: null // NodeLoader 完了後に接続
                property int _tmRev: 0
                // 根本的修正: 個別のパラメータ変更とフレーム移動の両方に反応する reactive なプロパティ
                readonly property var evaluatedParams: {
                    var _trig = clipNode._tmRev; // 変更検知トリガー
                    if (!Workspace.currentTimeline)
                        return {
                    };

                    // C++ から最新の計算済み（フラット化された）パラメータを取得
                    return Workspace.currentTimeline.evaluateClipParams(clipIdRole, root.currentFrame - clipStartFrameRole);
                }
                readonly property var tParams: {
                    var _ = clipNode._tmRev;
                    var tModel = null;
                    for (var i = 0; i < clipEffectModelsRole.length; i++) {
                        if (clipEffectModelsRole[i].id === "transform") {
                            tModel = clipEffectModelsRole[i];
                            break;
                        }
                    }
                    if (!tModel)
                        return {
                    };

                    var out = {
                    };
                    var fps = (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60;
                    var relFrame = root.currentFrame - clipStartFrameRole;
                    var keys = ["x", "y", "z", "rotationX", "rotationY", "rotationZ", "scale", "aspect", "opacity"];
                    for (var k = 0; k < keys.length; k++) {
                        var key = keys[k];
                        var v = tModel.evaluatedParam(key, relFrame, fps);
                        if (v === undefined || v === null)
                            v = tModel.params[key];

                        out[key] = v;
                    }
                    return out;
                }
                readonly property real px: tParams.x !== undefined ? Number(tParams.x) : 0
                readonly property real py: tParams.y !== undefined ? Number(tParams.y) : 0
                readonly property real pz: tParams.z !== undefined ? Number(tParams.z) : 0
                readonly property real pRotX: tParams.rotationX !== undefined ? Number(tParams.rotationX) : 0
                readonly property real pRotY: tParams.rotationY !== undefined ? Number(tParams.rotationY) : 0
                readonly property real pRotZ: tParams.rotationZ !== undefined ? Number(tParams.rotationZ) : 0
                readonly property real pScale: tParams.scale !== undefined ? Number(tParams.scale) : 100
                readonly property real pAspect: tParams.aspect !== undefined ? Number(tParams.aspect) : 0
                readonly property real pOpacity: tParams.opacity !== undefined ? Number(tParams.opacity) : 1
                // 拡大率と縦横比
                readonly property real baseScale: pScale * 0.01
                readonly property real aspectX: pAspect >= 0 ? (1 + pAspect) : 1
                readonly property real aspectY: pAspect < 0 ? (1 - pAspect) : 1
                // 実効トランスフォーム計算 (グループ制御適用)
                property var effectiveTransform: {
                    // 依存関係を明示的に登録 (groupControlsの変更を検知)
                    var _gcList = root.groupControls;
                    // 1. 適用可能なグループ制御を抽出
                    var activeGroups = [];
                    for (var i = 0; i < root.groupControls.length; ++i) {
                        var gc = root.groupControls[i];
                        if (!gc)
                            continue;

                        if (gc.clipLayer < clipLayerRole && clipLayerRole <= (gc.clipLayer + gc.layerCount))
                            activeGroups.push(gc);

                    }
                    // 2. レイヤー順（親→子）にソート
                    // 上位レイヤー(数値が小さい)ほど親として振る舞う
                    activeGroups.sort(function(a, b) {
                        return a.clipLayer - b.clipLayer;
                    });
                    // 3. 行列による階層的な座標計算
                    // グローバル座標系での変換行列を作成
                    var m = Qt.matrix4x4();
                    var totalOpacity = pOpacity;
                    var totalRotX = 0;
                    var totalRotY = 0;
                    var totalRotZ = 0;
                    // 親グループから順に変換を適用
                    for (var j = 0; j < activeGroups.length; ++j) {
                        var g = activeGroups[j];
                        // AviUtl互換座標系 (Y下プラス) で計算するため、Y軸反転などは最終出力時に行う
                        m.translate(Qt.vector3d(g.x, g.y, g.z));
                        m.rotate(g.rotationX, Qt.vector3d(1, 0, 0));
                        m.rotate(g.rotationY, Qt.vector3d(0, 1, 0));
                        m.rotate(g.rotationZ, Qt.vector3d(0, 0, 1));
                        var s = g.scale / 100;
                        m.scale(s, s, s);
                        totalOpacity *= g.opacity;
                        // 回転の単純加算（近似値。厳密な3D回転合成ではないが、AviUtl的な挙動としては十分）
                        totalRotX += g.rotationX;
                        totalRotY += g.rotationY;
                        totalRotZ += g.rotationZ;
                    }
                    // 最後にオブジェクト自身のローカル座標を適用
                    m.translate(Qt.vector3d(px, py, pz));
                    // 行列から最終的な位置を取得 (translation vector)
                    // column(3) は平行移動成分 (x, y, z, w)
                    var pos = m.column(3);
                    var t = {
                        "x": pos.x,
                        "y": pos.y,
                        "z": pos.z,
                        "rx": pRotX + totalRotX,
                        "ry": pRotY + totalRotY,
                        "rz": pRotZ + totalRotZ,
                        "sx": baseScale * aspectX,
                        "sy": baseScale * aspectY,
                        "sz": baseScale,
                        "opacity": totalOpacity
                    };
                    return t;
                }

                function dbg(msg) {
                    Logger.log("[CompositeView][clipId=" + clipIdRole + "][type=" + clipTypeRole + "] " + msg, Workspace.currentTimeline);
                }

                // レイヤーが非表示の場合は描画しない
                visible: {
                    // 1. レイヤー自体の表示設定
                    // QMLエンジンのバインディングシステムに検知させるため、layerStatesを直接参照する
                    var states = root.layerStates;
                    var layerInfo = (states !== undefined && states !== null) ? states[clipLayerRole] : null;
                    var layerVisible = (layerInfo !== null && layerInfo !== undefined) ? layerInfo.visible : true;
                    if (!layerVisible)
                        return false;

                    // 2. シーン ID の一致確認
                    if (root.sceneId !== -1 && clipSceneIdRole !== -1 && clipSceneIdRole !== root.sceneId)
                        return false;

                    // 3. 再生ヘッドがクリップの範囲内にあるか（プリロードされたオブジェクトを隠す）
                    var inTimeRange = root.currentFrame >= clipStartFrameRole && root.currentFrame < (clipStartFrameRole + clipDurationFramesRole);
                    if (inTimeRange)
                        console.log("[Debug] CompositeView: Clip", clipIdRole, "(Scene:" + clipSceneIdRole + ") VISIBLE at frame", root.currentFrame, "Pos:", Math.round(clipNode.x), Math.round(clipNode.y), "Scale:", clipNode.scale.x.toFixed(2));

                    return inTimeRange;
                }
                // 座標変換: 中心(0,0)、Y軸下プラス(AviUtl互換)
                // Qt3DはY上がプラスなので、入力を反転させる
                x: effectiveTransform.x
                y: -effectiveTransform.y
                z: effectiveTransform.z
                // 中心座標 (Pivot)
                pivot: Qt.vector3d(tParams.anchorX || 0, -(tParams.anchorY || 0), tParams.anchorZ || 0)
                // 3軸回転
                eulerRotation.x: effectiveTransform.rx
                eulerRotation.y: -effectiveTransform.ry
                eulerRotation.z: -effectiveTransform.rz
                scale.x: effectiveTransform.sx
                scale.y: effectiveTransform.sy
                scale.z: effectiveTransform.sz
                // 不透明度 (全体)
                opacity: effectiveTransform.opacity
                // params 変化 (scale/pos/rot/opacity) → BaseObject の fbCaptureItem を同期
                onPxChanged: objectContainer._syncTransformToItem()
                onPyChanged: objectContainer._syncTransformToItem()
                onPRotZChanged: objectContainer._syncTransformToItem()
                onBaseScaleChanged: objectContainer._syncTransformToItem()
                onAspectXChanged: objectContainer._syncTransformToItem()
                onAspectYChanged: objectContainer._syncTransformToItem()
                onPOpacityChanged: objectContainer._syncTransformToItem()

                // 根本的修正: 個別のパラメータ変更を clipsChanged なしで検知する
                Connections {
                    // これにより tParams や BaseObject 内部が再評価される

                    function onEffectParamChanged(clipId, effIdx, name, val) {
                        if (clipId === clipNode.clipIdRole)
                            clipNode._tmRev++;

                    }

                    target: Workspace.currentTimeline
                }

                Instantiator {
                    model: clipNode.clipEffectModelsRole

                    delegate: Connections {
                        function onParamsChanged() {
                            clipNode._tmRev++;
                        }

                        function onKeyframeTracksChanged() {
                            clipNode._tmRev++;
                        }

                        target: modelData
                        ignoreUnknownSignals: true
                    }

                }
                // 評価済みパラメータからtransform値を取得 (単一経路化)

                // Loader (2D) は 3D シーン内では機能しないため、
                // Qt.createComponent を使用して Node 派生クラスを動的に生成する
                Common.NodeLoader {
                    // 内部の evaluatedParams() 等の再計算を強制するトリガー

                    id: objectContainer

                    // clipNode の transform が変わるたびに BaseObject へ同期
                    function _syncTransformToItem() {
                        if (!item)
                            return ;

                        if ("clipNodeScaleX" in item)
                            item.clipNodeScaleX = clipNode.baseScale * clipNode.aspectX;

                        if ("clipNodeScaleY" in item)
                            item.clipNodeScaleY = clipNode.baseScale * clipNode.aspectY;

                        if ("clipNodePosX" in item)
                            item.clipNodePosX = clipNode.px;

                        if ("clipNodePosY" in item)
                            item.clipNodePosY = clipNode.py;

                        if ("clipNodeRotZ" in item)
                            item.clipNodeRotZ = clipNode.pRotZ;

                        if ("clipNodeOpacity" in item)
                            item.clipNodeOpacity = clipNode.pOpacity;

                    }

                    source: clipNode.clipQmlSourceRole
                    properties: {
                        "opacity": clipNode.pOpacity,
                        "clipId": clipNode.clipIdRole,
                        "clipStartFrame": clipNode.clipStartFrameRole,
                        "clipDurationFrames": clipNode.clipDurationFramesRole,
                        "revision": clipNode._tmRev,
                        "currentFrame": Qt.binding(function() {
                            return root.currentFrame;
                        }),
                        "rawEffectModels": Qt.binding(function() {
                            return clipNode.clipEffectModelsRole;
                        }),
                        "renderHost": offscreenRenderHost
                    }
                    componentFactory: root.getCachedComponent
                    // NodeLoader 完了後に fbRendererOutput を clipNode へ接続
                    onItemChanged: {
                        if (item) {
                            clipNode.fbRendererOutput = item.fbCaptureItem ?? null;
                            if ("clipLayer" in item)
                                item.clipLayer = clipNode.clipLayerRole;

                            if ("sceneId" in item)
                                item.sceneId = root.sceneId;

                            if ("rawEffectModels" in item)
                                item.rawEffectModels = Qt.binding(function() {
                                return clipNode.clipEffectModelsRole;
                            });

                            if ("currentFrame" in item)
                                item.currentFrame = Qt.binding(function() {
                                return root.currentFrame;
                            });

                            if (item.isGroupControl && root.registerGroupControl)
                                root.registerGroupControl(item);

                            _syncTransformToItem();
                        }
                        root.childRendererOutputsChanged();
                    }
                }

            }

        }

        environment: SceneEnvironment {
            id: sceneEnv

            backgroundMode: SceneEnvironment.Color
            clearColor: "#000000"
            antialiasingMode: SceneEnvironment.MSAA
            antialiasingQuality: SceneEnvironment.High
        }

    }

}
