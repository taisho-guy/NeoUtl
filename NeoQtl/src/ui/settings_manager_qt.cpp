#include "settings_manager_qt.h"
#include "NeoQtl/src/ui/timeline/bridge/timeline_bridge.cxx.h"

#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtQml/QQmlContext>
#include <QtQml/qqml.h>

namespace {

QString to_qstring(const rust::String &s) {
    return QString::fromUtf8(s.data(), static_cast<int>(s.size()));
}

rust::Str to_rust_str(const QByteArray &utf8) {
    return rust::Str(utf8.constData(), utf8.size());
}

QVariant unwrap(const rust::String &payload) {
    const QJsonArray wrapper = QJsonDocument::fromJson(to_qstring(payload).toUtf8()).array();
    return wrapper.isEmpty() ? QVariant() : wrapper.at(0).toVariant();
}

} 
SettingsManagerBridge::SettingsManagerBridge(QObject *parent) : QObject(parent) {}

QVariantMap SettingsManagerBridge::settings() const {
    const QByteArray utf8 = to_qstring(neoqtl::ui_settings_all()).toUtf8();
    return QJsonDocument::fromJson(utf8).object().toVariantMap();
}

QVariant SettingsManagerBridge::value(const QString &key, const QVariant &defaultValue) const {
    const QByteArray key_utf8 = key.toUtf8();
    const QVariant stored = unwrap(neoqtl::ui_settings_value(to_rust_str(key_utf8)));
    return stored.isNull() ? defaultValue : stored;
}

void SettingsManagerBridge::setValue(const QString &key, const QVariant &value) {
    const QByteArray key_utf8 = key.toUtf8();
    const QByteArray value_utf8 =
        QJsonDocument(QJsonArray::fromVariantList({value})).toJson(QJsonDocument::Compact);
    neoqtl::ui_settings_set(to_rust_str(key_utf8), to_rust_str(value_utf8));
    Q_EMIT settingsChanged();
}

bool SettingsManagerBridge::save() { return neoqtl::ui_settings_save(); }

void register_settings_manager_bridge(QQmlApplicationEngine &engine) {
    static SettingsManagerBridge instance;
    engine.rootContext()->setContextProperty(QStringLiteral("SettingsManager"), &instance);
}
