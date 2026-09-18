#include "bridge_qt.h"
#include "NeoQtl/src/ui/timeline/bridge/timeline_bridge.cxx.h"

#include <QtQml/qqml.h>
#include <QtQml/QQmlContext>

namespace {

QString to_qstring(const rust::String &s) {
    return QString::fromUtf8(s.data(), static_cast<int>(s.size()));
}

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

int TimelineBridge::currentFrame() const { return neoqtl::timeline_current_frame(); }
int TimelineBridge::totalFrames() const { return neoqtl::timeline_total_frames(); }
int TimelineBridge::layerCount() const { return neoqtl::timeline_layer_count(); }
qreal TimelineBridge::timelineScale() const { return neoqtl::timeline_scale(); }
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

void TimelineBridge::setViewport(qreal contentX, qreal contentY, qreal width, qreal height) {
    neoqtl::timeline_set_viewport(static_cast<float>(contentX), static_cast<float>(contentY),
                                  static_cast<float>(width), static_cast<float>(height));
}

void TimelineBridge::setZoom(qreal value) {
    neoqtl::timeline_set_zoom(static_cast<float>(value));
    Q_EMIT viewChanged();
}

void TimelineBridge::seek(int frame) {
    neoqtl::timeline_seek(frame);
    Q_EMIT transportChanged();
}

void TimelineBridge::selectClip(int id, bool additive) {
    neoqtl::timeline_select_clip(id, additive);
    Q_EMIT clipsChanged();
}

void TimelineBridge::clearSelection() {
    neoqtl::timeline_clear_selection();
    Q_EMIT clipsChanged();
}

void TimelineBridge::selectInRect(qreal x0, qreal y0, qreal x1, qreal y1) {
    neoqtl::timeline_select_in_rect(static_cast<float>(x0), static_cast<float>(y0),
                                    static_cast<float>(x1), static_cast<float>(y1));
    Q_EMIT clipsChanged();
}

void TimelineBridge::applyClipBatchMove(int clipId, int deltaLayer, int deltaStartFrame) {
    neoqtl::timeline_apply_clip_batch_move(clipId, deltaLayer, deltaStartFrame);
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::applyClipResize(int clipId, int deltaStartFrame, int deltaDuration) {
    neoqtl::timeline_apply_clip_resize(clipId, deltaStartFrame, deltaDuration);
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::splitClip(int id, int frame) {
    neoqtl::timeline_split_clip(id, frame);
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::deleteSelected() {
    neoqtl::timeline_delete_selected();
    Q_EMIT clipsChanged();
    Q_EMIT transportChanged();
}

void TimelineBridge::moveKeyframe(int id, int fromFrame, int toFrame) {
    neoqtl::timeline_move_keyframe(id, fromFrame, toFrame);
    Q_EMIT clipsChanged();
}

bool TimelineBridge::switchScene(int sceneId) {
    const bool ok = neoqtl::timeline_switch_scene(sceneId);
    if (ok) {
        Q_EMIT scenesChanged();
        Q_EMIT clipsChanged();
        Q_EMIT transportChanged();
        Q_EMIT sceneSettingsChanged();
    }
    return ok;
}

int TimelineBridge::addScene(const QString &name) {
    const QByteArray utf8 = name.toUtf8();
    const int id = neoqtl::timeline_add_scene(rust::Str(utf8.constData(), utf8.size()));
    Q_EMIT scenesChanged();
    return id;
}

bool TimelineBridge::removeScene(int sceneId) {
    const bool ok = neoqtl::timeline_remove_scene(sceneId);
    if (ok) {
        Q_EMIT scenesChanged();
        Q_EMIT clipsChanged();
    }
    return ok;
}

void register_timeline_bridge(QQmlApplicationEngine &engine) {
    static TimelineBridge instance;
    engine.rootContext()->setContextProperty(QStringLiteral("TimelineBridge"), &instance);
}
