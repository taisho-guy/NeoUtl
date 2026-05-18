import QtQuick
import QtQuick.Effects

Item {
    id: base

    // ObjectRenderer から Binding で注入される
    property var params
    property Item source
    // ソースアイテムを非表示にしつつ、テクスチャとして利用可能にするプロキシ
    property alias sourceProxy: proxySource
    property QtObject effectModel
    property int frame: 0
    // QMLバインディング再評価用（params/keyframes変更を確実に検知）
    property int _rev: 0

    // 【統一API】キーフレーム優先評価（ECS同期）
    function evalParam(key, fallback) {
        var _ = base._rev;
        if (base.effectModel) {
            var fps = (Workspace.currentTimeline && Workspace.currentTimeline.project) ? Workspace.currentTimeline.project.fps : 60;
            var v = base.effectModel.evaluatedParam(key, base.frame, fps);
            if (v !== undefined && v !== null)
                return v;

        }
        if (base.params && base.params[key] !== undefined)
            return base.params[key];

        return fallback;
    }

    // 数値専用（型変換込み）
    function evalNumber(key, fallback) {
        return Number(evalParam(key, fallback));
    }

    // 色専用
    function evalColor(key, fallback) {
        var v = evalParam(key, fallback);
        return (typeof v === 'string') ? v : fallback;
    }

    ShaderEffectSource {
        id: proxySource

        sourceItem: base.source
        hideSource: true
        visible: true
        opacity: 0
    }

    Connections {
        function onParamChanged(key, value) {
            base._rev++;
        }

        function onParamsChanged() {
            base._rev++;
        }

        function onKeyframeTracksChanged() {
            base._rev++;
        }

        target: base.effectModel
    }

}
