.pragma library

var _debugChecked = false;
var _debugEnabled = false;

function isDebugEnabled() {
    if (!_debugChecked) {
        _debugEnabled = (Qt.application.arguments.indexOf("--aviqtl-debug") !== -1);
        _debugChecked = true;
    }
    return _debugEnabled;
}

function log(msg, bridge) {
    if (!isDebugEnabled()) return;
    console.log(msg);
    if (bridge && typeof bridge.log === 'function') {
        bridge.log(msg);
    }
}