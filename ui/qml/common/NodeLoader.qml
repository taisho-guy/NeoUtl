import "Logger.js" as Logger
import QtQuick
import QtQuick3D

Node {
    id: loader

    property url source
    property var properties: ({
    })
    property QtObject item: null
    // [FIX-01] item が差し替わる直前の旧オブジェクトを保持する。
    // CompositeView.onItemChanged で旧アイテムの unregister を確実に行うために必要。
    // このプロパティで旧参照を渡すことで SIGSEGV を防ぐ。
    property QtObject previousItem: null
    property int status: Loader.Null
    property string errorString: ""
    // 外部からコンポーネント取得関数を注入可能にする
    // function(url) -> Component
    property var componentFactory: null
    property Component _component: null

    function _applyProperties() {
        if (!item || !properties)
            return ;

        for (var key in properties) {
            if (item.hasOwnProperty(key) || (key in item))
                item[key] = properties[key];

        }
    }

    function _load() {
        // [FIX-01] 旧 item を previousItem に退避してから null 化する。
        // CompositeView 側の onItemChanged ハンドラが previousItem を参照して
        // unregisterCameraControl / unregisterGroupControl を安全に呼べるようにする。
        if (item) {
            previousItem = item;
            item = null;
            // [FIX-02] previousItem.destroy() は非同期（次のイベントループ）なので、
            // Qt.callLater で退避後に破棄する。これにより onItemChanged が先に
            Qt.callLater(function() {
                if (previousItem) {
                    previousItem.destroy();
                    // [FIX-03] 破棄完了後に previousItem をクリアして dangling 参照を除去する。
                    previousItem = null;
                }
            });
        }
        _component = null;
        status = Loader.Null;
        errorString = "";
        if (source == "") {
            status = Loader.Null;
            return ;
        }
        var comp = null;
        if (componentFactory && typeof componentFactory === "function")
            comp = componentFactory(source);
        else
            comp = Qt.createComponent(source, Component.Asynchronous);
        _component = comp;
        _processStatus();
    }

    function _processStatus() {
        if (!_component)
            return ;

        status = _component.status;
        if (status === Component.Ready) {
            item = _component.createObject(loader);
            _applyProperties();
        } else if (status === Component.Error) {
            errorString = _component.errorString();
            Logger.log(qsTr("[NodeLoader] コンポーネントエラー: %1").arg(errorString));
        }
    }

    onSourceChanged: _load()
    onPropertiesChanged: _applyProperties()

    Connections {
        function onStatusChanged() {
            loader._processStatus();
        }

        target: _component
    }

}
