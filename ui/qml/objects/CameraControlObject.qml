import QtQuick
import QtQuick3D
import "qrc:/qt/qml/AviQtl/ui/qml/common" as Common

// CameraControlObject は BaseObject を継承しない。
// 理由: BaseObject は adopt2D() で sourceItem/renderer を offscreenRenderHost
//       出ると Qt3D 内部でヌルポインタ参照が発生し SIGSEGV する。
// そのため Node を直接継承し、シーングラフ内に留まる最小実装とする。
Node {
    id: root

    property bool isCameraControl: true
    property Item renderHost: null
    property int clipId: -1
    property int sceneId: -1
    property int clipStartFrame: 0
    property int clipDurationFrames: 0
    property int clipLayer: -1
    property int currentFrame: 0
    property var rawEffectModels: []
    property int _tmRev: 0
    property alias revision: root._tmRev
    property Item fbCaptureItem: _dummyCapture
    // [FIX-04] 登録済みフラグ。重複登録と登録漏れを両方防ぐ。
    // cameraControls[] に複数の同一参照が混入すると activeCameraControl の
    // 評価ループが破棄済みオブジェクトを複数回参照し SIGSEGV する。
    property bool _registered: false
    readonly property int relFrame: currentFrame - clipStartFrame
    readonly property real projectFps: (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60
    property int layerCount: Math.max(1, Number(evalParam("camera", "layerCount", 1)))
    property real camX: Number(evalParam("camera", "x", 0))
    property real camY: Number(evalParam("camera", "y", 0))
    property real camZ: Number(evalParam("camera", "z", 0))
    property real tarX: Number(evalParam("camera", "tx", 0))
    property real tarY: Number(evalParam("camera", "ty", 0))
    property real tarZ: Number(evalParam("camera", "tz", 0))
    property real roll: Number(evalParam("camera", "roll", 0))
    property real fov: Math.max(1, Math.min(170, Number(evalParam("camera", "fov", 30))))
    // CompositeView が View3D.camera に直接バインドするノード
    property alias camera: cam
    readonly property real _defaultDist: {
        var h = (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.height : 1080;
        return h / (2 * Math.tan(fov * Math.PI / 360));
    }

    function evalParam(effectId, paramName, fallback) {
        var _ = root._tmRev; // リアクティブ依存
        if (root.rawEffectModels) {
            for (var i = 0; i < root.rawEffectModels.length; i++) {
                var eff = root.rawEffectModels[i];
                if (eff.id === effectId) {
                    if (eff.evaluatedParam) {
                        var v = eff.evaluatedParam(paramName, root.relFrame, root.projectFps);
                        if (v !== undefined && v !== null)
                            return v;

                    }
                    if (eff.params && eff.params[paramName] !== undefined)
                        return eff.params[paramName];

                }
            }
        }
        return fallback;
    }

    // [FIX-05] 登録ヘルパー関数。CompositeView の参照を直接保持せず、
    // renderHost.parent 経由での動的解決を毎回行う。
    // これにより renderHost が差し替わった場合の古い CompositeView への
    // 誤登録を防ぐ。
    function _tryRegister() {
        if (_registered)
            return ;

        // [FIX-06] renderHost.parent が Item であることを確認してから呼ぶ。
        // CompositeView が破棄済みの場合は null チェックで弾く。
        if (renderHost && renderHost.parent && typeof renderHost.parent.registerCameraControl === "function") {
            renderHost.parent.registerCameraControl(root);
            _registered = true;
        }
    }

    // [FIX-07] 解除ヘルパー関数。_registered フラグで二重解除を防ぐ。
    // 呼び出し先の CompositeView が有効かどうかを先に確認する。
    function _tryUnregister() {
        if (!_registered)
            return ;

        if (renderHost && renderHost.parent && typeof renderHost.parent.unregisterCameraControl === "function")
            renderHost.parent.unregisterCameraControl(root);

        // [FIX-08] unregister の成否にかかわらずフラグをクリアする。
        // CompositeView が先に破棄された場合でも dangling 参照として
        // cameraControls[] に残り続けるのを防ぐ。
        _registered = false;
    }

    // [FIX-09] NodeLoader 側の onItemChanged で registerCameraControl が呼ばれるため
    // ここでは _tryRegister() をフォールバックとして残す。
    // NodeLoader.onItemChanged → CompositeView.onItemChanged の順で登録されるので
    // 二重登録は _registered フラグで防御済み。
    Component.onCompleted: {
        Qt.callLater(_tryRegister);
    }
    // [FIX-10] Component.onDestruction は QML エンジンが既にオブジェクトを
    // 破棄しつつある状態で呼ばれる。このタイミングでは renderHost や
    // renderHost.parent が既に null である可能性が高い。
    // ここでは「まだ登録が残っていた場合の最終防衛」として機能する。
    Component.onDestruction: {
        // renderHost.parent への安全なアクセスのみ試みる
        // （フラグが残っていれば unregister、なければ何もしない）
        _tryUnregister();
    }
    // [FIX-11] renderHost が後から注入される（または差し替えられる）ケースに対応。
    // renderHost が変わったタイミングで古い CompositeView から解除し、
    // 新しい CompositeView に登録し直す。
    onRenderHostChanged: {
        _tryUnregister();
        Qt.callLater(_tryRegister);
    }

    Instantiator {
        model: root.rawEffectModels

        Connections {
            function onParamsChanged() {
                root._tmRev++;
            }

            function onKeyframeTracksChanged() {
                root._tmRev++;
            }

            target: modelData
            ignoreUnknownSignals: true
        }

    }

    // BaseObject の adopt2D() による移送は行わない。
    PerspectiveCamera {
        id: cam

        readonly property vector3d _target: Qt.vector3d(root.tarX, -root.tarY, root.tarZ)
        readonly property vector3d _dir: {
            var d = position.minus(_target);
            var len = Math.sqrt(d.x * d.x + d.y * d.y + d.z * d.z);
            return len > 0 ? Qt.vector3d(d.x / len, d.y / len, d.z / len) : Qt.vector3d(0, 0, 1);
        }

        fieldOfView: root.fov
        clipFar: 5000
        // AviUtl 座標系 (Y下プラス) → Qt3D (Y上プラス): Y を反転
        position: Qt.vector3d(root.camX, -root.camY, root._defaultDist + root.camZ)
        eulerRotation: {
            var d = _dir;
            var pitch = Math.asin(-d.y) * 180 / Math.PI;
            var yaw = Math.atan2(d.x, d.z) * 180 / Math.PI;
            return Qt.vector3d(pitch, yaw, root.roll);
        }
    }

    Item {
        id: _dummyCapture

        width: 1
        height: 1
        visible: false
    }

}
