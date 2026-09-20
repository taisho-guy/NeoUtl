#pragma once

#include <QtCore/QObject>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtQml/QQmlApplicationEngine>

class SettingsManagerBridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap settings READ settings NOTIFY settingsChanged)

public:
    explicit SettingsManagerBridge(QObject *parent = nullptr);

    QVariantMap settings() const;

    Q_INVOKABLE QVariant value(const QString &key, const QVariant &defaultValue) const;
    Q_INVOKABLE void setValue(const QString &key, const QVariant &value);
    Q_INVOKABLE bool save();

Q_SIGNALS:
    void settingsChanged();
};

void register_settings_manager_bridge(QQmlApplicationEngine &engine);
