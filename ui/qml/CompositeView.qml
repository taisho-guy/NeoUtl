import QtQml
import QtQuick
import QtQuick3D
import "common" as Common
import "common/Logger.js" as Logger

Item {
    id: root

    property var layerStates: ({
    })
    property var clipModel: []
    property int projectWidth: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.width : 1920
    property int projectHeight: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.height : 1080
    property int currentFrame: (Workspace.currentTimeline && Workspace.currentTimeline.transport) ? Workspace.currentTimeline.transport.currentFrame : 0
    property int sceneId: -1
    property var sceneStack: sceneId >= 0 ? [sceneId] : []
    readonly property int hiddenZ: -9999
    property var _componentCache: ({
    })
    property bool exportMode: false
    property alias view3D: view
    property var groupControls: []
    property var cameraControls: []
    // [FIX-12] activeCameraControl の評価で破棄済みオブジェクトを触らないよう
    // isCameraControl フラグを確認してから clipLayer にアクセスする。
    // QML オブジェクトが破棄されると各プロパティへのアクセスは undefined を返すが、
    // メソッド呼び出しは SIGSEGV を引き起こすため、フラグ確認が安全弁となる。
    readonly property var activeCameraControl: {
        var _cc = root.cameraControls; // バインディングトリガー
        if (!_cc || _cc.length === 0)
            return null;

        var best = null;
        for (var i = 0; i < _cc.length; ++i) {
            var cc = _cc[i];
            // [FIX-13] null / 破棄済みオブジェクトを二重チェック。
            // isCameraControl が true であることは「生きているオブジェクト」の証明。
            // 破棄済みオブジェクトは QML エンジンが null を返すか、
            // プロパティアクセスが undefined になる。
            if (!cc || cc.isCameraControl !== true)
                continue;

            if (best === null || cc.clipLayer < best.clipLayer)
                best = cc;

        }
        return best;
    }

    signal childRendererOutputsChanged()

    function getLayerVisible(layer) {
        if (!layerStates)
            return true;

        var state = layerStates[layer];
        return state !== undefined ? state.visible : true;
    }

    // [FIX-14] push() による直接変異を廃止し、新配列への代入でバインディングを
    // 確実に更新する。push/splice はリアクティブバインディングに通知されない
    // ケースがあり、cameraControlsChanged() の手動発火に依存するのは脆弱。
    function registerCameraControl(cc) {
        if (!cc || cc.isCameraControl !== true)
            return ;

        // 重複チェック
        for (var i = 0; i < cameraControls.length; ++i) {
            if (cameraControls[i] === cc)
                return ;

        }
        // [FIX-15] 新配列への代入。QML バインディングエンジンが変更を検知できる。
        cameraControls = cameraControls.concat([cc]);
    }

    function unregisterCameraControl(cc) {
        // [FIX-16] filter() で新配列を生成。splice() のような破壊的変異を避ける。
        // cc が null でも filter 内で === 比較するだけなので安全。
        var next = cameraControls.filter(function(x) {
            return x !== cc;
        });
        if (next.length !== cameraControls.length)
            cameraControls = next;

    }

    function registerGroupControl(gc) {
        for (var i = 0; i < groupControls.length; ++i) {
            if (groupControls[i] === gc)
                return ;

        }
        groupControls = groupControls.concat([gc]);
    }

    function unregisterGroupControl(gc) {
        var next = groupControls.filter(function(x) {
            return x !== gc;
        });
        if (next.length !== groupControls.length)
            groupControls = next;

    }

    function getCachedComponent(url) {
        if (_componentCache[url])
            return _componentCache[url];

        var c = Qt.createComponent(url, Component.Asynchronous);
        _componentCache[url] = c;
        return c;
    }

    Item {
        id: offscreenRenderHost

        width: SettingsManager ? SettingsManager.value("maxImageSize", 8192) : 8192
        height: SettingsManager ? SettingsManager.value("maxImageSize", 8192) : 8192
        anchors.centerIn: parent
        visible: true
        opacity: 0
        enabled: false
        z: hiddenZ
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        z: -2
    }

    View3D {
        id: view

        property int projW: root.projectWidth
        property int projH: root.projectHeight
        property double aspect: projW / projH
        property double currentClipTimeRatio: (Workspace.currentTimeline) ? Math.max(0, Math.min(1, (root.currentFrame - Workspace.currentTimeline.clipStartFrame) / Workspace.currentTimeline.clipDurationFrames)) : 0

        // [FIX-17] activeCameraControl.camera へのアクセス前に camera プロパティの
        // typeof チェックで安全に fallback する。
        camera: {
            var acc = root.activeCameraControl;
            if (acc && typeof acc.camera !== "undefined" && acc.camera)
                return acc.camera;

            return mainCamera;
        }
        width: root.exportMode ? projW : Math.min(parent.width, parent.height * aspect)
        height: root.exportMode ? projH : Math.min(parent.height, parent.width / aspect)
        anchors.centerIn: parent
        focus: true
        Keys.onSpacePressed: {
            if (Workspace.currentTimeline && Workspace.currentTimeline.transport)
                Workspace.currentTimeline.transport.togglePlay();

        }

        Rectangle {
            anchors.fill: parent
            color: "#0a0a0a"
            z: -1
        }

        PerspectiveCamera {
            id: mainCamera

            property real distance: view.projH / (2 * Math.tan(fieldOfView * Math.PI / 360))

            fieldOfView: 30
            position: Qt.vector3d(0, 0, distance)
            clipFar: 5000
        }

        DirectionalLight {
            eulerRotation.x: -30
            z: 1000
        }

        Node {
            Model {
                source: "#Rectangle"
                scale: Qt.vector3d(root.projectWidth / 100, root.projectHeight / 100, 1)
                position: Qt.vector3d(0, 0, -10)
                visible: false

                materials: DefaultMaterial {
                    diffuseColor: "#22000000"
                }

            }

        }

        Node {
            id: sceneRoot
        }

        Instantiator {
            model: root.clipModel
            onObjectAdded: (index, object) => {
                object.parent = sceneRoot;
            }
            onObjectRemoved: (index, object) => {
                object.parent = null;
            }

            delegate: Node {
                id: clipNode

                property var _clipData: (typeof modelData !== "undefined") ? modelData : model
                property int clipIdRole: _clipData.id
                property int clipSceneIdRole: _clipData.sceneId !== undefined ? _clipData.sceneId : -1
                property string clipTypeRole: _clipData.type
                property int clipLayerRole: _clipData.layer
                property int clipStartFrameRole: _clipData.startFrame
                property int clipDurationFramesRole: _clipData.durationFrames
                property url clipQmlSourceRole: _clipData.qmlSource || ""
                property var clipEffectModelsRole: _clipData.effectModels || []
                property Item fbRendererOutput: null
                property int _tmRev: 0
                property bool clipByUpperObjectRole: Boolean(_clipData.clipByUpperObject)
                property Item rawFbRendererOutput: null
                readonly property bool clipByUpperActive: clipByUpperObjectRole && clipMaskItem && rawFbRendererOutput
                readonly property Item clippedRendererOutput: clipByUpperLoader.item ? clipByUpperLoader.item.output : null
                property Item clipMaskItem: null
                readonly property var evaluatedParams: {
                    var _trig = clipNode._tmRev;
                    if (!Workspace.currentTimeline)
                        return {
                    };

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
                readonly property real baseScale: pScale * 0.01
                readonly property real aspectX: pAspect >= 0 ? (1 + pAspect) : 1
                readonly property real aspectY: pAspect < 0 ? (1 - pAspect) : 1 // Note: pAspect<0 の時は 1+|pAspect| ではなく元実装に合わせる
                property var effectiveTransform: {
                    var _gcList = root.groupControls;
                    var activeGroups = [];
                    for (var i = 0; i < root.groupControls.length; ++i) {
                        var gc = root.groupControls[i];
                        if (!gc)
                            continue;

                        if (gc.clipLayer < clipLayerRole && clipLayerRole <= (gc.clipLayer + gc.layerCount))
                            activeGroups.push(gc);

                    }
                    activeGroups.sort(function(a, b) {
                        return a.clipLayer - b.clipLayer;
                    });
                    var m = Qt.matrix4x4();
                    var totalOpacity = pOpacity;
                    var totalRotX = 0, totalRotY = 0, totalRotZ = 0;
                    for (var j = 0; j < activeGroups.length; ++j) {
                        var g = activeGroups[j];
                        m.translate(Qt.vector3d(g.x, g.y, g.z));
                        m.rotate(g.rotationX, Qt.vector3d(1, 0, 0));
                        m.rotate(g.rotationY, Qt.vector3d(0, 1, 0));
                        m.rotate(g.rotationZ, Qt.vector3d(0, 0, 1));
                        var s = g.scale / 100;
                        m.scale(s, s, s);
                        totalOpacity *= g.opacity;
                        totalRotX += g.rotationX;
                        totalRotY += g.rotationY;
                        totalRotZ += g.rotationZ;
                    }
                    m.translate(Qt.vector3d(px, py, pz));
                    var pos = m.column(3);
                    return {
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
                }

                function dbg(msg) {
                    Logger.log("[CompositeView][clipId=" + clipIdRole + "][type=" + clipTypeRole + "] " + msg, Workspace.currentTimeline);
                }

                function isClipActiveAtCurrentFrame(node) {
                    if (!node || node.clipStartFrameRole === undefined || node.clipDurationFramesRole === undefined)
                        return false;

                    var start = Number(node.clipStartFrameRole);
                    var duration = Number(node.clipDurationFramesRole);
                    return root.currentFrame >= start && root.currentFrame < start + duration;
                }

                function findUpperClipMask() {
                    if (!clipByUpperObjectRole || clipLayerRole <= 0 || !sceneRoot)
                        return null;

                    var bestLayer = -1, bestMask = null;
                    var nodes = sceneRoot.children;
                    for (var i = 0; i < nodes.length; ++i) {
                        var node = nodes[i];
                        if (!node || node === clipNode)
                            continue;

                        if (node.clipLayerRole === undefined || node.clipLayerRole >= clipLayerRole || node.clipLayerRole <= bestLayer)
                            continue;

                        if (node.clipSceneIdRole !== clipSceneIdRole)
                            continue;

                        if (!node.visible || !isClipActiveAtCurrentFrame(node) || !node.rawFbRendererOutput)
                            continue;

                        bestLayer = node.clipLayerRole;
                        bestMask = node.rawFbRendererOutput;
                    }
                    return bestMask;
                }

                function updateClipMaskItem() {
                    clipMaskItem = findUpperClipMask();
                }

                Component.onCompleted: Qt.callLater(updateClipMaskItem)
                onClipStartFrameRoleChanged: Qt.callLater(updateClipMaskItem)
                onClipDurationFramesRoleChanged: Qt.callLater(updateClipMaskItem)
                onClipByUpperObjectRoleChanged: Qt.callLater(updateClipMaskItem)
                onClipLayerRoleChanged: Qt.callLater(updateClipMaskItem)
                onClipSceneIdRoleChanged: Qt.callLater(updateClipMaskItem)
                onVisibleChanged: {
                    root.childRendererOutputsChanged();
                    Qt.callLater(updateClipMaskItem);
                }
                onRawFbRendererOutputChanged: Qt.callLater(updateClipMaskItem)
                onFbRendererOutputChanged: root.childRendererOutputsChanged()
                visible: {
                    var states = root.layerStates;
                    var layerInfo = (states !== undefined && states !== null) ? states[clipLayerRole] : null;
                    var layerVisible = (layerInfo !== null && layerInfo !== undefined) ? layerInfo.visible : true;
                    if (!layerVisible)
                        return false;

                    if (root.sceneId !== -1 && clipSceneIdRole !== -1 && clipSceneIdRole !== root.sceneId)
                        return false;

                    return root.currentFrame >= clipStartFrameRole && root.currentFrame < (clipStartFrameRole + clipDurationFramesRole);
                }
                x: effectiveTransform.x
                y: effectiveTransform.y
                z: effectiveTransform.z
                pivot: Qt.vector3d(tParams.anchorX || 0, tParams.anchorY || 0, tParams.anchorZ || 0)
                eulerRotation.x: effectiveTransform.rx
                eulerRotation.y: -effectiveTransform.ry
                eulerRotation.z: -effectiveTransform.rz
                scale.x: effectiveTransform.sx
                scale.y: effectiveTransform.sy
                scale.z: effectiveTransform.sz
                opacity: effectiveTransform.opacity
                onPxChanged: objectContainer._syncTransformToItem()
                onPyChanged: objectContainer._syncTransformToItem()
                onPRotZChanged: objectContainer._syncTransformToItem()
                onBaseScaleChanged: objectContainer._syncTransformToItem()
                onAspectXChanged: objectContainer._syncTransformToItem()
                onAspectYChanged: objectContainer._syncTransformToItem()
                onPOpacityChanged: objectContainer._syncTransformToItem()
                onEffectiveTransformChanged: objectContainer._syncTransformToItem()

                Connections {
                    function onCurrentFrameChanged() {
                        Qt.callLater(clipNode.updateClipMaskItem);
                    }

                    function onChildRendererOutputsChanged() {
                        Qt.callLater(clipNode.updateClipMaskItem);
                    }

                    target: root
                }

                Binding {
                    target: clipNode
                    property: "fbRendererOutput"
                    value: clipNode.rawFbRendererOutput
                }

                Loader {
                    id: clipByUpperLoader

                    parent: offscreenRenderHost
                    active: clipNode.clipByUpperActive
                    sourceComponent: clipByUpperComponent
                }

                Component {
                    id: clipByUpperComponent

                    Item {
                        property alias output: clippedCapture

                        width: root.projectWidth
                        height: root.projectHeight
                        visible: true

                        ShaderEffect {
                            id: clipByUpperEffect

                            property variant source
                            property variant maskSource

                            anchors.fill: parent
                            fragmentShader: AviQtlAssetUrl + "/effects/clip_by_upper_object.frag.qsb"

                            source: ShaderEffectSource {
                                sourceItem: clipNode.rawFbRendererOutput
                                live: true
                                recursive: false
                                hideSource: false
                            }

                            maskSource: ShaderEffectSource {
                                sourceItem: clipNode.clipMaskItem
                                live: true
                                recursive: false
                                hideSource: false
                            }

                        }

                        ShaderEffectSource {
                            id: clippedCapture

                            anchors.fill: parent
                            sourceItem: clipByUpperEffect
                            live: true
                            hideSource: false
                            recursive: false
                            format: ShaderEffectSource.RGBA
                        }

                    }

                }

                Connections {
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

                Common.NodeLoader {
                    id: objectContainer

                    function _syncTransformToItem() {
                        if (!item)
                            return ;

                        if ("clipNodeScaleX" in item)
                            item.clipNodeScaleX = clipNode.effectiveTransform.sx;

                        if ("clipNodeScaleY" in item)
                            item.clipNodeScaleY = clipNode.effectiveTransform.sy;

                        if ("clipNodePosX" in item)
                            item.clipNodePosX = clipNode.effectiveTransform.x;

                        if ("clipNodePosY" in item)
                            item.clipNodePosY = clipNode.effectiveTransform.y;

                        if ("clipNodeRotZ" in item)
                            item.clipNodeRotZ = clipNode.effectiveTransform.rz;

                        if ("clipNodeOpacity" in item)
                            item.clipNodeOpacity = clipNode.effectiveTransform.opacity;

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
                    onPreviousItemChanged: {
                        // [FIX-18] NodeLoader が旧 item を previousItem に退避した直後に
                        // unregister を同期的に実行する。これが SIGSEGV の根本修正。
                        // destroy() より前に unregister を済ませることで、
                        // cameraControls[] に dangling 参照が残らない。
                        var prev = previousItem;
                        if (prev) {
                            if (prev.isCameraControl && root.unregisterCameraControl)
                                root.unregisterCameraControl(prev);

                            if (prev.isGroupControl && root.unregisterGroupControl)
                                root.unregisterGroupControl(prev);

                        }
                    }
                    onItemChanged: {
                        if (item) {
                            clipNode.rawFbRendererOutput = item.fbCaptureItem ?? null;
                            if ("outputModelVisible" in item)
                                item.outputModelVisible = Qt.binding(function() {
                                return true;
                            });

                            if ("displayOutput" in item)
                                item.displayOutput = Qt.binding(function() {
                                return clipNode.clipByUpperActive && clipNode.clippedRendererOutput ? clipNode.clippedRendererOutput : item.renderer ? item.renderer.output : null;
                            });

                            if ("clipLayer" in item)
                                item.clipLayer = clipNode.clipLayerRole;

                            if ("sceneId" in item)
                                item.sceneId = root.sceneId;

                            if ("sceneRootRef" in item)
                                item.sceneRootRef = sceneRoot;

                            if ("sceneStack" in item)
                                item.sceneStack = Qt.binding(function() {
                                return root.sceneStack;
                            });

                            if ("rawEffectModels" in item)
                                item.rawEffectModels = Qt.binding(function() {
                                return clipNode.clipEffectModelsRole;
                            });

                            if ("currentFrame" in item)
                                item.currentFrame = Qt.binding(function() {
                                return root.currentFrame;
                            });

                            // [FIX-19] isGroupControl チェックは item が生きていることの確認。
                            // previousItem の unregister が onPreviousItemChanged で
                            if (item.isGroupControl && root.registerGroupControl)
                                root.registerGroupControl(item);

                            // [FIX-20] registerCameraControl は内部で重複チェックを行う。
                            // CameraControlObject 側の _tryRegister() と合わせて
                            if (item.isCameraControl && root.registerCameraControl)
                                root.registerCameraControl(item);

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
