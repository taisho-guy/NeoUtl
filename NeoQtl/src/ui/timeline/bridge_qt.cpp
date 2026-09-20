#include "bridge_qt.h"
#include "NeoQtl/src/ui/timeline/bridge/timeline_bridge.cxx.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtQml/QQmlContext>
#include <QtQml/qqml.h>

namespace {

QString to_qstring(const rust::String &s) {
    return QString::fromUtf8(s.data(), static_cast<int>(s.size()));
}

rust::Str to_rust_str(const QByteArray &utf8) {
    return rust::Str(utf8.constData(), utf8.size());
}

QByteArray encode_args(const QVariantList &args) {
    return QJsonDocument(QJsonArray::fromVariantList(args)).toJson(QJsonDocument::Compact);
}

QVariant decode_result(const rust::String &payload) {
    const QByteArray utf8 = to_qstring(payload).toUtf8();
    const QJsonArray wrapper = QJsonDocument::fromJson(utf8).array();
    return wrapper.isEmpty() ? QVariant() : wrapper.at(0).toVariant();
}

QVariant query(const char *name, const QVariantList &args = {}) {
    const QByteArray name_utf8(name);
    const QByteArray args_utf8 = encode_args(args);
    return decode_result(neoqtl::timeline_query(to_rust_str(name_utf8), to_rust_str(args_utf8)));
}

QVariant invoke(const char *name, const QVariantList &args = {}) {
    const QByteArray name_utf8(name);
    const QByteArray args_utf8 = encode_args(args);
    return decode_result(neoqtl::timeline_invoke(to_rust_str(name_utf8), to_rust_str(args_utf8)));
}

} 
TransportBridge::TransportBridge(QObject *parent) : QObject(parent) {}

bool TransportBridge::isPlaying() const { return query("transport").toMap()["isPlaying"].toBool(); }
bool TransportBridge::isScrubbing() const { return query("transport").toMap()["isScrubbing"].toBool(); }
int TransportBridge::currentFrame() const { return query("transport").toMap()["currentFrame"].toInt(); }
int TransportBridge::totalFrames() const { return query("transport").toMap()["totalFrames"].toInt(); }
qreal TransportBridge::playbackSpeed() const { return query("transport").toMap()["playbackSpeed"].toDouble(); }

void TransportBridge::togglePlay() {
    invoke("togglePlay");
    Q_EMIT transportChanged();
}

void TransportBridge::pause() {
    invoke("pause");
    Q_EMIT transportChanged();
}

void TransportBridge::setCurrentFrame_seek(int frame) {
    invoke("seek", {frame});
    Q_EMIT transportChanged();
}

void TransportBridge::scrubTo(int frame) {
    invoke("scrubTo", {frame});
    Q_EMIT transportChanged();
}

void TransportBridge::beginScrub() {
    invoke("beginScrub");
    Q_EMIT transportChanged();
}

void TransportBridge::endScrub() {
    invoke("endScrub");
    Q_EMIT transportChanged();
}

TransportBridge *transport_bridge_instance() {
    static TransportBridge instance;
    return &instance;
}

TimelineBridge::TimelineBridge(QObject *parent) : QObject(parent) {}

QVariantList TimelineBridge::clips() const {
    QVariantList out;
    for (const auto &c : neoqtl::timeline_clips()) {
        QVariantMap m;
        m["id"] = c.id;
        m["startFrame"] = c.start_frame;
        m["durationFrames"] = c.duration_frames;
        m["layer"] = c.layer;
        m["colorIndex"] = c.color_index;
        m["type"] = c.color_index;
        m["kindKnown"] = c.kind_known;
        m["selected"] = c.selected;
        m["locked"] = c.locked;
        m["label"] = to_qstring(c.label);
        QVariantList kf;
        for (const auto f : c.keyframe_frames) {
            kf.append(f);
        }
        m["keyframes"] = kf;
        out.append(m);
    }
    return out;
}

QVariantList TimelineBridge::sceneTabs() const {
    QVariantList out;
    for (const auto &s : neoqtl::timeline_scene_tabs()) {
        QVariantMap m;
        m["id"] = s.id;
        m["name"] = to_qstring(s.name);
        m["active"] = s.active;
        out.append(m);
    }
    return out;
}

QVariantList TimelineBridge::scenes() const { return query("scenes").toList(); }

int TimelineBridge::currentFrame() const { return neoqtl::timeline_current_frame(); }
int TimelineBridge::totalFrames() const { return neoqtl::timeline_total_frames(); }
int TimelineBridge::layerCount() const { return neoqtl::timeline_layer_count(); }
qreal TimelineBridge::timelineScale() const { return neoqtl::timeline_scale(); }

void TimelineBridge::setTimelineScale(qreal value) {
    neoqtl::timeline_set_zoom(static_cast<float>(value));
    Q_EMIT viewChanged();
}

int TimelineBridge::projectFps() const { return neoqtl::timeline_project_fps(); }
bool TimelineBridge::enableSnap() const { return neoqtl::timeline_enable_snap(); }
int TimelineBridge::magneticSnapRange() const { return neoqtl::timeline_magnetic_snap_range(); }

QVariantMap TimelineBridge::gridSettings() const {
    const auto g = neoqtl::timeline_grid_settings();
    QVariantMap m;
    m["mode"] = to_qstring(g.mode);
    m["bpm"] = g.bpm;
    m["offset"] = g.offset;
    m["interval"] = g.interval;
    m["subdivision"] = g.subdivision;
    return m;
}

QObject *TimelineBridge::transport() const { return transport_bridge_instance(); }
QVariantMap TimelineBridge::project() const { return query("project").toMap(); }
QVariantMap TimelineBridge::selection() const { return query("selection").toMap(); }
QVariantList TimelineBridge::previewSelectionIds() const { return query("previewSelectionIds").toList(); }
int TimelineBridge::cursorFrame() const { return query("cursorFrame").toInt(); }

void TimelineBridge::setCursorFrame(int frame) {
    neoqtl::timeline_seek(frame);
    Q_EMIT transportChanged();
}

int TimelineBridge::selectedLayer() const { return query("selectedLayer").toInt(); }

void TimelineBridge::setSelectedLayer(int layer) {
    invoke("setSelectedLayer", {layer});
    Q_EMIT selectionChanged();
}

int TimelineBridge::timelineDuration() const { return query("timelineDuration").toInt(); }
int TimelineBridge::currentSceneId() const { return query("currentSceneId").toInt(); }
int TimelineBridge::clipStartFrame() const { return query("clipStartFrame").toInt(); }
int TimelineBridge::clipDurationFrames() const { return query("clipDurationFrames").toInt(); }
QString TimelineBridge::currentProjectUrl() const { return query("currentProjectUrl").toString(); }
bool TimelineBridge::hasUnsavedChanges() const { return query("hasUnsavedChanges").toBool(); }
bool TimelineBridge::isExporting() const { return query("isExporting").toBool(); }

void TimelineBridge::setViewport(qreal contentX, qreal contentY, qreal width, qreal height) {
    neoqtl::timeline_set_viewport(static_cast<float>(contentX), static_cast<float>(contentY),
                                  static_cast<float>(width), static_cast<float>(height));
}

void TimelineBridge::updateViewport(qreal contentX, qreal contentY) {
    invoke("updateViewport", {contentX, contentY});
    Q_EMIT viewChanged();
}

void TimelineBridge::setZoom(qreal value) {
    neoqtl::timeline_set_zoom(static_cast<float>(value));
    Q_EMIT viewChanged();
}

void TimelineBridge::seek(int frame) {
    neoqtl::timeline_seek(frame);
    Q_EMIT transportChanged();
}

void TimelineBridge::togglePlay() {
    invoke("togglePlay");
    Q_EMIT transportChanged();
}

void TimelineBridge::syncPlaybackSpeed() {
    invoke("syncPlaybackSpeed");
    Q_EMIT transportChanged();
}

void TimelineBridge::setCompositeView(bool enabled) { invoke("setCompositeView", {enabled}); }

void TimelineBridge::selectClip(int id, bool additive) {
    neoqtl::timeline_select_clip(id, additive);
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::clearSelection() {
    neoqtl::timeline_clear_selection();
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::selectInRect(qreal x0, qreal y0, qreal x1, qreal y1) {
    neoqtl::timeline_select_in_rect(static_cast<float>(x0), static_cast<float>(y0),
                                    static_cast<float>(x1), static_cast<float>(y1));
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::applySelectionIds(const QVariantList &ids) {
    invoke("applySelectionIds", {ids});
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::handleClipClick(int id, int modifiers) {
    invoke("handleClipClick", {id, modifiers});
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::updateSelectionPreview(int frame0, int frame1, int layer0, int layer1,
                                            bool additive) {
    invoke("updateSelectionPreview", {frame0, frame1, layer0, layer1, additive});
    Q_EMIT selectionChanged();
}

void TimelineBridge::clearSelectionPreview() {
    invoke("clearSelectionPreview");
    Q_EMIT selectionChanged();
}

void TimelineBridge::finalizeSelectionPreview() {
    invoke("finalizeSelectionPreview");
    Q_EMIT clipsChanged();
    Q_EMIT selectionChanged();
}

void TimelineBridge::applyClipBatchMove(const QVariant &moves) {
    invoke("applyClipBatchMove", {moves});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::applyClipResize(int clipId, int deltaStartFrame, int deltaDuration) {
    neoqtl::timeline_apply_clip_resize(clipId, deltaStartFrame, deltaDuration);
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::updateClip(int clipId, int layer, int startFrame, int durationFrames) {
    invoke("updateClip", {clipId, layer, startFrame, durationFrames});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::deleteClip(int clipId) {
    invoke("deleteClip", {clipId});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::deleteSelected() {
    neoqtl::timeline_delete_selected();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::deleteSelectedClips() {
    invoke("deleteSelectedClips");
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::copyClip(int clipId) { invoke("copyClip", {clipId}); }
void TimelineBridge::copySelectedClips() { invoke("copySelectedClips"); }

void TimelineBridge::cutClip(int clipId) {
    invoke("cutClip", {clipId});
    Q_EMIT clipsChanged();
}

void TimelineBridge::cutSelectedClips() {
    invoke("cutSelectedClips");
    Q_EMIT clipsChanged();
}

QVariantList TimelineBridge::pasteClip(int frame, int layer) {
    const QVariantList ids = invoke("pasteClip", {frame, layer}).toList();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
    return ids;
}

void TimelineBridge::splitClip(int id, int frame) {
    neoqtl::timeline_split_clip(id, frame);
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::splitSelectedClips(int frame) {
    invoke("splitSelectedClips", {frame});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::moveSelectedClips(int deltaFrame, int deltaLayer) {
    invoke("moveSelectedClips", {deltaFrame, deltaLayer});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::resizeSelectedClips(int deltaStartFrame, int deltaDuration) {
    invoke("resizeSelectedClips", {deltaStartFrame, deltaDuration});
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

QVariantMap TimelineBridge::resolveDragDelta(int clipId, int deltaFrame, int deltaLayer,
                                             const QVariantList &activeIds, int selectionMinFrame,
                                             int selectionMinLayer, int selectionMaxLayer,
                                             int layerCount) {
    return query("resolveDragDelta", {clipId, deltaFrame, deltaLayer, activeIds, selectionMinFrame,
                                      selectionMinLayer, selectionMaxLayer, layerCount})
        .toMap();
}

bool TimelineBridge::clipByUpperObject(int clipId) const {
    return query("clipByUpperObject", {clipId}).toBool();
}

void TimelineBridge::setClipByUpperObject(int clipId, bool enabled) {
    invoke("setClipByUpperObject", {clipId, enabled});
    Q_EMIT clipsChanged();
}

QString TimelineBridge::getClipTypeColor(int type) const {
    return query("getClipTypeColor", {type}).toString();
}

bool TimelineBridge::isAudioClip(int clipId) const {
    return query("isAudioClip", {clipId}).toBool();
}

void TimelineBridge::setKeyframe(int clipId, int effectIndex, const QString &key, int frame,
                                 qreal value, const QVariantMap &options) {
    invoke("setKeyframe", {clipId, effectIndex, key, frame, value, options});
    Q_EMIT clipsChanged();
}

void TimelineBridge::removeKeyframe(int clipId, int effectIndex, const QString &key, int frame) {
    invoke("removeKeyframe", {clipId, effectIndex, key, frame});
    Q_EMIT clipsChanged();
}

void TimelineBridge::moveKeyframe(int clipId, int effectIndex, const QString &key, int fromFrame,
                                  int toFrame) {
    invoke("moveKeyframe", {clipId, effectIndex, key, fromFrame, toFrame});
    Q_EMIT clipsChanged();
}

QVariantList TimelineBridge::getWaveformPeaks(int clipId, int width, int durationFrames) const {
    return query("getWaveformPeaks", {clipId, width, durationFrames}).toList();
}

QVariantList TimelineBridge::getAvailableEffects() const { return query("getAvailableEffects").toList(); }

QVariantList TimelineBridge::getClipEffectStack(int clipId) const {
    return query("getClipEffectStack", {clipId}).toList();
}

QVariantList TimelineBridge::getClipEffectsModel(int clipId) const {
    return query("getClipEffectsModel", {clipId}).toList();
}

int TimelineBridge::getClipEffectIndex(int clipId, const QString &effectId) const {
    return query("getClipEffectIndex", {clipId, effectId}).toInt();
}

QVariantList TimelineBridge::getEffectParameters(int clipId, int effectIndex) const {
    return query("getEffectParameters", {clipId, effectIndex}).toList();
}

void TimelineBridge::addEffect(int clipId, const QString &effectId) {
    invoke("addEffect", {clipId, effectId});
    Q_EMIT effectsChanged();
}

void TimelineBridge::removeEffect(int clipId, int effectIndex) {
    invoke("removeEffect", {clipId, effectIndex});
    Q_EMIT effectsChanged();
}

void TimelineBridge::removeMultipleEffects(int clipId, const QVariantList &effectIndices) {
    invoke("removeMultipleEffects", {clipId, effectIndices});
    Q_EMIT effectsChanged();
}

void TimelineBridge::reorderEffects(int clipId, int fromIndex, int toIndex) {
    invoke("reorderEffects", {clipId, fromIndex, toIndex});
    Q_EMIT effectsChanged();
}

void TimelineBridge::reorderMultipleEffects(int clipId, const QVariantList &fromIndices,
                                            int toIndex) {
    invoke("reorderMultipleEffects", {clipId, fromIndices, toIndex});
    Q_EMIT effectsChanged();
}

void TimelineBridge::setEffectEnabled(int clipId, int effectIndex, bool enabled) {
    invoke("setEffectEnabled", {clipId, effectIndex, enabled});
    Q_EMIT effectsChanged();
}

void TimelineBridge::setEffectParameter(int clipId, int effectIndex, const QString &key,
                                        qreal value) {
    invoke("setEffectParameter", {clipId, effectIndex, key, value});
    Q_EMIT effectsChanged();
}

void TimelineBridge::updateClipEffectParam(int clipId, int effectIndex, const QString &key,
                                           qreal value) {
    invoke("updateClipEffectParam", {clipId, effectIndex, key, value});
    Q_EMIT effectsChanged();
}

bool TimelineBridge::addAudioPlugin(int clipId, const QString &pluginId) {
    const bool ok = invoke("addAudioPlugin", {clipId, pluginId}).toBool();
    Q_EMIT effectsChanged();
    return ok;
}

void TimelineBridge::removeAudioPlugin(int clipId, int pluginIndex) {
    invoke("removeAudioPlugin", {clipId, pluginIndex});
    Q_EMIT effectsChanged();
}

void TimelineBridge::setAudioPluginEnabled(int clipId, int pluginIndex, bool enabled) {
    invoke("setAudioPluginEnabled", {clipId, pluginIndex, enabled});
    Q_EMIT effectsChanged();
}

void TimelineBridge::reorderAudioPlugins(int clipId, int fromIndex, int toIndex) {
    invoke("reorderAudioPlugins", {clipId, fromIndex, toIndex});
    Q_EMIT effectsChanged();
}

QVariantList TimelineBridge::getPluginCategories() const {
    return query("getPluginCategories").toList();
}

QVariantList TimelineBridge::getPluginsByCategory(const QString &category) const {
    return query("getPluginsByCategory", {category}).toList();
}

void TimelineBridge::updateAudioSampleRate() { invoke("updateAudioSampleRate"); }

bool TimelineBridge::isLayerHidden(int layer) const { return neoqtl::timeline_layer_hidden(layer); }
bool TimelineBridge::isLayerLocked(int layer) const { return neoqtl::timeline_layer_locked(layer); }

void TimelineBridge::setLayerState(int layer, bool value, int field) {
    invoke("setLayerState", {layer, value, field});
    Q_EMIT clipsChanged();
}

void TimelineBridge::insertLayers(int layerIndex, int count, bool above) {
    invoke("insertLayers", {layerIndex, count, above});
    Q_EMIT clipsChanged();
}

void TimelineBridge::shiftLayers(int fromLayer, int toLayer, int delta) {
    invoke("shiftLayers", {fromLayer, toLayer, delta});
    Q_EMIT clipsChanged();
}

bool TimelineBridge::switchScene(int sceneId) {
    const bool ok = neoqtl::timeline_switch_scene(sceneId);
    if (ok) {
        Q_EMIT scenesChanged();
        Q_EMIT clipsChanged();
        Q_EMIT transportChanged();
        Q_EMIT sceneSettingsChanged();
        Q_EMIT currentSceneIdChanged();
    }
    return ok;
}

int TimelineBridge::addScene(const QString &name) {
    const QByteArray utf8 = name.toUtf8();
    const int id = neoqtl::timeline_add_scene(rust::Str(utf8.constData(), utf8.size()));
    Q_EMIT scenesChanged();
    return id;
}

int TimelineBridge::createScene(const QString &name) { return addScene(name); }

bool TimelineBridge::removeScene(int sceneId) {
    const bool ok = neoqtl::timeline_remove_scene(sceneId);
    if (ok) {
        Q_EMIT scenesChanged();
        Q_EMIT clipsChanged();
    }
    return ok;
}

QVariantList TimelineBridge::getSceneClips(int sceneId) const {
    return query("getSceneClips", {sceneId}).toList();
}

int TimelineBridge::getSceneDuration(int sceneId) const {
    return query("getSceneDuration", {sceneId}).toInt();
}

QVariantMap TimelineBridge::getSceneInfo(int sceneId) const {
    return query("getSceneInfo", {sceneId}).toMap();
}

bool TimelineBridge::updateSceneSettings(int sceneId, const QString &name, int width, int height,
                                         qreal fps, int totalFrames, int gridMode, qreal bpm) {
    const bool ok =
        invoke("updateSceneSettings", {sceneId, name, width, height, fps, totalFrames, gridMode, bpm})
            .toBool();
    Q_EMIT sceneSettingsChanged();
    Q_EMIT scenesChanged();
    Q_EMIT transportChanged();
    return ok;
}

QVariantList TimelineBridge::getAvailableObjects() const {
    return query("getAvailableObjects").toList();
}

int TimelineBridge::createObject(const QString &stableId, int frame, int layer) {
    const int id = invoke("createObject", {stableId, frame, layer}).toInt();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
    return id;
}

bool TimelineBridge::saveProject(const QString &path) {
    const bool ok = invoke("saveProject", {path}).toBool();
    Q_EMIT projectChanged();
    return ok;
}

bool TimelineBridge::undo() {
    const bool ok = invoke("undo").toBool();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
    Q_EMIT selectionChanged();
    return ok;
}

bool TimelineBridge::redo() {
    const bool ok = invoke("redo").toBool();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
    Q_EMIT selectionChanged();
    return ok;
}

bool TimelineBridge::exportVideoAsync(const QVariantMap &options) {
    const bool ok = invoke("exportVideoAsync", {options}).toBool();
    Q_EMIT exportChanged();
    return ok;
}

bool TimelineBridge::cancelExport() {
    const bool ok = invoke("cancelExport").toBool();
    Q_EMIT exportChanged();
    return ok;
}

TimelineBridge *timeline_bridge_instance() {
    static TimelineBridge instance;
    return &instance;
}

void register_timeline_bridge(QQmlApplicationEngine &engine) {
    engine.rootContext()->setContextProperty(QStringLiteral("TimelineBridge"),
                                             timeline_bridge_instance());
}
