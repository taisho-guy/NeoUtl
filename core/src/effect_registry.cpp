#include "effect_registry.hpp"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSet>
#include <QUrl>

namespace AviQtl::Core {

namespace {
constexpr const char *kMetadataTranslationContext = "AviQtl::Core::EffectRegistry";

auto translatedMetadataString(const QString &source) -> QString {
    if (source.isEmpty()) {
        return source;
    }

    return QCoreApplication::translate(kMetadataTranslationContext, source.toUtf8().constData());
}

auto localizeUiMetadataValue(const QVariant &value) -> QVariant {
    if (value.metaType().id() == QMetaType::QVariantMap) {
        QVariantMap map = value.toMap();
        static const QSet<QString> translatableKeys = {
            QStringLiteral("label"),
            QStringLiteral("title"),
            QStringLiteral("text"),
            QStringLiteral("name"),
            QStringLiteral("filter"),
            QStringLiteral("placeholder"),
            QStringLiteral("unit"),
        };

        for (auto it = map.begin(); it != map.end(); ++it) {
            if (it.key() == QStringLiteral("options") && it.value().metaType().id() == QMetaType::QVariantList) {
                QVariantList options;
                const QVariantList rawOptions = it.value().toList();
                for (const QVariant &option : rawOptions) {
                    if (option.metaType().id() == QMetaType::QString) {
                        const QString rawText = option.toString();
                        QVariantMap displayOption;
                        displayOption.insert(QStringLiteral("value"), rawText);
                        displayOption.insert(QStringLiteral("label"), translatedMetadataString(rawText));
                        options.append(displayOption);
                    } else {
                        options.append(localizeUiMetadataValue(option));
                    }
                }
                it.value() = options;
            } else if (it.value().metaType().id() == QMetaType::QString && translatableKeys.contains(it.key())) {
                it.value() = translatedMetadataString(it.value().toString());
            } else {
                it.value() = localizeUiMetadataValue(it.value());
            }
        }
        return map;
    }

    if (value.metaType().id() == QMetaType::QVariantList) {
        QVariantList list = value.toList();
        for (QVariant &entry : list) {
            entry = localizeUiMetadataValue(entry);
        }
        return list;
    }

    return value;
}
} // namespace

void EffectRegistry::loadEffectsFromDirectory(const QString &path) {
    QDir dir(path);
    if (!dir.exists()) {
        qWarning().noquote() << QStringLiteral("Effect directory not found:") << path;
        return;
    }

    // *.json ファイルをサブディレクトリを含めて検索
    QDirIterator it(path, {QStringLiteral("*.json")}, QDir::Files, QDirIterator::Subdirectories);

    while (it.hasNext()) {
        QFile file(it.next());
        if (!file.open(QIODevice::ReadOnly)) {
            continue;
        }

        QJsonParseError error;
        // 最適化: 巨大なJSONの場合はメモリマップドファイル化を検討
        const auto data = file.readAll();
        const auto doc = QJsonDocument::fromJson(data, &error);

        if (error.error != QJsonParseError::NoError || !doc.isObject()) {
            qWarning().noquote() << QStringLiteral("Invalid JSON in") << file.fileName() << QStringLiteral(":") << error.errorString();
            continue;
        }

        QJsonObject obj = doc.object();
        QString id = obj.value(QStringLiteral("id")).toString();
        QString name = obj.value(QStringLiteral("name")).toString();
        QString qmlFileName = obj.value(QStringLiteral("qml")).toString();
        QVariantMap params = obj.value(QStringLiteral("params")).toObject().toVariantMap();
        QVariantMap uiDef = obj.value(QStringLiteral("ui")).toObject().toVariantMap();

        if (!uiDef.contains(QStringLiteral("controls"))) {
            qWarning().noquote() << QStringLiteral("不正な定義のためスキップ (ui.controls なし):") << file.fileName();
            continue;
        }

        QString version = obj.value(QStringLiteral("version")).toString();
        QRegularExpression versionRegex(QStringLiteral("^\\d+\\.\\d+\\.\\d+$"));
        if (!versionRegex.match(version).hasMatch()) {
            qWarning().noquote() << QStringLiteral("versionの形式が不正または存在しません (x.x.x必須):") << file.fileName();
            continue;
        }

        QString kind = obj.value(QStringLiteral("kind")).toString();
        if (kind != QStringLiteral("effect") && kind != QStringLiteral("object")) {
            qWarning().noquote() << QStringLiteral("kindが不正です ('effect'または'object'必須):") << file.fileName();
            continue;
        }

        QStringList categories;
        QJsonArray catArray = obj.value(QStringLiteral("categories")).toArray();
        for (const QJsonValue &val : catArray) {
            if (val.isString()) {
                categories.append(val.toString());
            }
        }
        if (categories.isEmpty()) {
            qWarning().noquote() << QStringLiteral("categoriesが空または存在しません (1つ以上のカテゴリ必須):") << file.fileName();
            continue;
        }

        if (id.isEmpty() || name.isEmpty() || qmlFileName.isEmpty()) {
            qWarning().noquote() << QStringLiteral("不完全な定義のためスキップ:") << file.fileName();
            continue;
        }

        EffectMetadata meta;
        meta.version = version;
        meta.id = id;
        meta.name = translatedMetadataString(name);
        meta.kind = kind;
        for (QString &category : categories) {
            category = translatedMetadataString(category);
        }
        meta.categories = categories;
        meta.defaultParams = params;
        meta.uiDefinition = localizeUiMetadataValue(uiDef).toMap();
        meta.color = obj.value(QStringLiteral("color")).toString();

        // qrc: で始まる場合は絶対パスとしてそのまま使用
        if (qmlFileName.startsWith(QStringLiteral("qrc:"))) {
            meta.qmlSource = qmlFileName;
            registerEffect(meta);
            qDebug().noquote() << QStringLiteral("外部エフェクトをロード:") << name << QStringLiteral("(") << id << QStringLiteral(") from") << file.fileName();
            continue;
        }

        // QMLファイルの絶対パスを解決 (JSONファイルからの相対パスとして処理)
        QFileInfo jsonInfo(file.fileName());
        QString absoluteQmlPath = jsonInfo.absoluteDir().filePath(qmlFileName);

        if (QFile::exists(absoluteQmlPath)) {
            meta.qmlSource = QUrl::fromLocalFile(absoluteQmlPath).toString();
        } else {
            qWarning() << QStringLiteral("参照されているQMLファイルが見つかりません。エフェクト:") << id << QStringLiteral("パス:") << absoluteQmlPath;
            continue;
        }

        registerEffect(meta);
        qDebug().noquote() << QStringLiteral("外部エフェクトをロード:") << name << QStringLiteral("(") << id << QStringLiteral(") from") << file.fileName();
    }
}

} // namespace AviQtl::Core
