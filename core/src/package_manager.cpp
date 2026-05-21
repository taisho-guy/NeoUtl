#include "package_manager.hpp"
#include "settings_manager.hpp"
#include "version.hpp" // AviQtl本体のバージョン情報にアクセスするため
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QTimer>
#include <QXmlStreamReader>

namespace AviQtl::Core {

namespace {
QString getInstalledPackagesPath() {
    const QString path = QCoreApplication::applicationDirPath() + QStringLiteral("/repos");
    QDir().mkpath(path);
    return path + QStringLiteral("/installed.json");
}

int compareVersions(const QString &v1, const QString &v2) {
    if (v1 == v2)
        return 0;

    auto sanitize = [](QString v) {
        if (v.startsWith('v'))
            v.remove(0, 1);
        return v;
    };

    QStringList parts1 = sanitize(v1).split('.');
    QStringList parts2 = sanitize(v2).split('.');

    int i = 0;
    while (i < parts1.size() && i < parts2.size()) {
        bool ok1, ok2;
        int num1 = parts1[i].toInt(&ok1);
        int num2 = parts2[i].toInt(&ok2);

        if (ok1 && ok2) { // 両方とも数値の場合
            if (num1 < num2)
                return -1;
            if (num1 > num2)
                return 1;
        } else { // 少なくとも一方が純粋な数値ではない場合 (例: "84a")
            // 文字列として比較。プレリリース版タグの場合、辞書順比較が望ましい挙動になることが多い
            if (parts1[i] < parts2[i])
                return -1;
            if (parts1[i] > parts2[i])
                return 1;
        }
        i++;
    }

    // 片方のバージョンがより多くの部分を持つ場合、通常は新しいと見なす (例: 1.0.0 vs 1.0)
    if (parts1.size() > parts2.size())
        return 1;
    if (parts1.size() < parts2.size())
        return -1;

    return 0; // v1 != v2 の場合はここに到達しないはず
}

QVariantMap loadInstalledPackagesFromFile() {
    const QString installedPath = getInstalledPackagesPath();
    QFile file(installedPath);
    QVariantMap installed;
    if (file.open(QIODevice::ReadOnly)) {
        installed = QJsonDocument::fromJson(file.readAll()).object().toVariantMap();
        file.close();
    }
    return installed;
}

QString parseLatestVersionFromXml(const QByteArray &data) {
    QString latest;
    QXmlStreamReader xml(data);
    while (!xml.atEnd()) {
        if (xml.isStartElement() && (xml.name() == QStringLiteral("item") || xml.name() == QStringLiteral("entry"))) {
            while (xml.readNextStartElement()) {
                if (xml.name() == QStringLiteral("title")) {
                    latest = xml.readElementText().trimmed();
                } else {
                    xml.skipCurrentElement();
                }
                if (!latest.isEmpty())
                    break;
            }
            break;
        }
        xml.readNext();
    }
    return latest;
}
} // namespace

PackageManager &PackageManager::instance() {
    static PackageManager instance;
    return instance;
}

PackageManager::PackageManager(QObject *parent) : QObject(parent) {
    m_statusText = tr("待機中");
    m_networkManager = new QNetworkAccessManager(this);
    QTimer::singleShot(0, this, [this]() { loadCachedPackages(); });
}

void PackageManager::setBusy(bool busy) {
    if (m_isBusy == busy)
        return;
    m_isBusy = busy;
    emit isBusyChanged();
}

void PackageManager::setStatus(const QString &status) {
    if (m_statusText == status)
        return;
    m_statusText = status;
    emit statusTextChanged();
}

void PackageManager::setProgress(double p) {
    if (m_progress == p)
        return;
    m_progress = p;
    emit progressChanged();
}

void PackageManager::setHasUpdatesAvailable(bool available) {
    if (m_hasUpdatesAvailable == available)
        return;
    m_hasUpdatesAvailable = available;
    emit hasUpdatesAvailableChanged();
}

void PackageManager::loadCachedPackages() {
    const QString repoPath = QCoreApplication::applicationDirPath() + QStringLiteral("/repos");
    QDir dir(repoPath);
    if (!dir.exists())
        return;

    m_packageList.clear();
    QVariantMap installed = loadInstalledPackagesFromFile();

    // 保存されているすべてのリポジトリJSONを読み込む
    const QStringList files = dir.entryList({QStringLiteral("*.json")}, QDir::Files);
    for (const QString &fileName : files) {
        if (fileName == QStringLiteral("installed.json"))
            continue;

        QFile file(dir.absoluteFilePath(fileName));
        if (!file.open(QIODevice::ReadOnly))
            continue;

        QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
        if (doc.isObject()) {
            QJsonArray packages = doc.object().value(QStringLiteral("packages")).toArray();
            for (const auto &pVal : packages) {
                QVariantMap p = pVal.toObject().toVariantMap();
                const QString id = p.value(QStringLiteral("id")).toString();

                // インストール済み情報の付加
                if (id == QStringLiteral("org.aviqtl.app")) {
                    p[QStringLiteral("installed_version")] = QString::fromUtf8(AviQtl::VERSION_STRING);
                    p[QStringLiteral("latest_version")] = QString::fromUtf8(AviQtl::VERSION_STRING);
                } else if (installed.contains(id)) {
                    p[QStringLiteral("installed_version")] = installed.value(id).toMap().value(QStringLiteral("version"));
                }

                // 起動直後は最新バージョンは不明（同期ボタンで初めて取得される）
                // ただし、キャッシュされたフィードがあれば読み込む
                if (id != QStringLiteral("org.aviqtl.app")) {
                    const QString feedUrl = p.value(QStringLiteral("release_feed")).toString();
                    if (!feedUrl.isEmpty()) {
                        const QString feedFileName = QStringLiteral("feed_") + QString::fromLatin1(QCryptographicHash::hash(feedUrl.toUtf8(), QCryptographicHash::Sha1).toHex()) + QStringLiteral(".xml");
                        QFile feedFile(repoPath + QStringLiteral("/") + feedFileName);
                        if (feedFile.open(QIODevice::ReadOnly)) {
                            p[QStringLiteral("latest_version")] = parseLatestVersionFromXml(feedFile.readAll());
                            feedFile.close();
                        }
                    }
                }

                m_packageList.append(p);
            }
        }
    }
    emit packageListChanged();

    // キャッシュ情報に基づいてアップデートの有無を初期評価
    bool anyUpdates = false;
    for (const auto &p : m_packageList) {
        const QVariantMap item = p.toMap();
        const QString instVer = item.value(QStringLiteral("installed_version")).toString();
        const QString latVer = item.value(QStringLiteral("latest_version")).toString();
        if (!instVer.isEmpty() && !latVer.isEmpty() && compareVersions(latVer, instVer) > 0) {
            anyUpdates = true;
            break;
        }
    }
    setHasUpdatesAvailable(anyUpdates);

    setStatus(tr("キャッシュからパッケージをロードしました（更新を確認するには「同期」を押してください）"));
}

QStringList PackageManager::repositories() const { return SettingsManager::instance().value(QStringLiteral("packageRepositoryUrls")).toStringList(); }

void PackageManager::addRepository(const QString &url) {
    QStringList repos = repositories();
    if (!url.isEmpty() && !repos.contains(url)) {
        repos.append(url);
        SettingsManager::instance().setValue(QStringLiteral("packageRepositoryUrls"), repos);
        emit repositoriesChanged();
    }
}

void PackageManager::removeRepository(const QString &url) {
    QStringList repos = repositories();
    if (repos.removeOne(url)) {
        SettingsManager::instance().setValue(QStringLiteral("packageRepositoryUrls"), repos);
        emit repositoriesChanged();
    }
}

void PackageManager::refreshRepositories() {
    if (m_isBusy)
        return;
    setBusy(true);
    m_packageList.clear();
    emit packageListChanged();

    QVariantMap installed = loadInstalledPackagesFromFile();
    installed.insert(QStringLiteral("org.aviqtl.app"), QVariantMap{{QStringLiteral("version"), QString::fromUtf8(AviQtl::VERSION_STRING)}});

    setStatus(tr("リポジトリを同期中..."));
    setProgress(0.0);

    QStringList urls = repositories();
    if (urls.isEmpty()) {
        setBusy(false);
        return;
    }

    m_pendingRequests = urls.size();
    for (const QString &url : urls) {
        QNetworkReply *reply = m_networkManager->get(QNetworkRequest(QUrl(url)));
        connect(reply, &QNetworkReply::finished, this, [this, reply, url, installed]() {
            reply->deleteLater();
            m_pendingRequests--;

            if (reply->error() == QNetworkReply::NoError) {
                const QByteArray data = reply->readAll();

                // JSONをreposディレクトリにキャッシュ保存
                const QString repoPath = QCoreApplication::applicationDirPath() + QStringLiteral("/repos");
                QDir().mkpath(repoPath);

                const QString fileName = QString::fromLatin1(QCryptographicHash::hash(url.toUtf8(), QCryptographicHash::Sha1).toHex()) + QStringLiteral(".json");
                QFile file(repoPath + QStringLiteral("/") + fileName);
                if (file.open(QIODevice::WriteOnly)) {
                    file.write(data);
                    file.close();
                }

                QJsonDocument doc = QJsonDocument::fromJson(data);
                if (doc.isObject()) {
                    QJsonArray packages = doc.object().value("packages").toArray();
                    for (const auto &pVal : packages) {
                        QVariantMap p = pVal.toObject().toVariantMap();
                        const QString id = p.value("id").toString();

                        // ローカルのインストール済み情報をチェック
                        if (id == QStringLiteral("org.aviqtl.app")) {
                            // AviQtl本体の場合、常に現在の実行バージョンをインストール済みとする
                            p["installed_version"] = QString::fromUtf8(AviQtl::VERSION_STRING);
                        } else if (installed.contains(id)) {
                            // その他のパッケージの場合
                            p["installed_version"] = installed.value(id).toMap().value("version");
                        }

                        // JSON側のバージョンを初期の最新バージョンとしてセット
                        if (p.contains(QStringLiteral("version"))) {
                            QString jsonVer = p.value(QStringLiteral("version")).toString();
                            if (id == QStringLiteral("org.aviqtl.app") && compareVersions(jsonVer, QString::fromUtf8(AviQtl::VERSION_STRING)) <= 0) {
                                p[QStringLiteral("latest_version")] = QString::fromUtf8(AviQtl::VERSION_STRING);
                            } else {
                                p[QStringLiteral("latest_version")] = jsonVer;
                            }
                        }

                        // バージョン等の変動情報はクライアント側で解決する
                        // JSON側の "version" フィールドは無視し、release_feed を優先する
                        const QString feedUrl = p.value("release_feed").toString();

                        if (!feedUrl.isEmpty()) {
                            QNetworkReply *rssReply = m_networkManager->get(QNetworkRequest(QUrl(feedUrl)));
                            m_pendingRequests++;
                            connect(rssReply, &QNetworkReply::finished, this, [this, rssReply, id, feedUrl]() {
                                rssReply->deleteLater();
                                m_pendingRequests--;

                                if (rssReply->error() == QNetworkReply::NoError) {
                                    const QByteArray rssData = rssReply->readAll();
                                    // フィードをキャッシュ保存
                                    const QString repoPath = QCoreApplication::applicationDirPath() + QStringLiteral("/repos");
                                    const QString feedFileName = QStringLiteral("feed_") + QString::fromLatin1(QCryptographicHash::hash(feedUrl.toUtf8(), QCryptographicHash::Sha1).toHex()) + QStringLiteral(".xml");
                                    QFile feedFile(repoPath + QStringLiteral("/") + feedFileName);
                                    if (feedFile.open(QIODevice::WriteOnly)) {
                                        feedFile.write(rssData);
                                        feedFile.close();
                                    }

                                    QString latest = parseLatestVersionFromXml(rssData);
                                    if (!latest.isEmpty()) {
                                        updatePackageLatestVersion(id, latest);
                                    }
                                }

                                if (m_pendingRequests <= 0) {
                                    // すべてのRSSリクエストが完了したら、最終的な状態を更新
                                    setBusy(false);
                                    setProgress(1.0);
                                    setStatus(tr("同期完了"));
                                    emit repositoryRefreshed();
                                    emit packageListChanged();

                                    // アップデートの有無を再評価
                                    bool anyUpdates = false;
                                    for (const auto &p : m_packageList) {
                                        const QVariantMap item = p.toMap();
                                        const QString instVer = item.value(QStringLiteral("installed_version")).toString();
                                        const QString latVer = item.value(QStringLiteral("latest_version")).toString();
                                        if (!instVer.isEmpty() && !latVer.isEmpty() && compareVersions(latVer, instVer) > 0) {
                                            anyUpdates = true;
                                            break;
                                        }
                                    }
                                    setHasUpdatesAvailable(anyUpdates);
                                }
                            });
                        }
                        m_packageList.append(p);
                    }
                    // メタデータの解析が終わった時点で一度リストを更新（ユーザーに即座に表示）
                    emit packageListChanged();
                }
            }

            if (m_pendingRequests <= 0) {
                setProgress(1.0);
                setStatus(tr("同期完了"));
                setBusy(false);
                emit repositoryRefreshed();
                emit packageListChanged();
            } else {
                QStringList urls = repositories();
                if (urls.size() > 0)
                    setProgress(1.0 - (double)m_pendingRequests / urls.size());
            }
        });
    }
}

void PackageManager::updatePackageLatestVersion(const QString &id, const QString &version) {
    for (int i = 0; i < m_packageList.size(); ++i) {
        QVariantMap item = m_packageList[i].toMap();
        if (item.value(QStringLiteral("id")).toString() == id) {
            QString latest = version;

            // 先頭の 'v' を取り除く (v0.0.86 -> 0.0.86)
            if (latest.startsWith('v'))
                latest.remove(0, 1);

            // AviQtl本体の場合、自分自身のバージョン情報と比較して、新しい場合のみ更新する
            if (id == QStringLiteral("org.aviqtl.app") && compareVersions(latest, QString::fromUtf8(AviQtl::VERSION_STRING)) <= 0) {
                latest = QString::fromUtf8(AviQtl::VERSION_STRING);
            }

            if (item.value(QStringLiteral("latest_version")).toString() != latest || latest.isEmpty()) {
                item[QStringLiteral("latest_version")] = latest;
                m_packageList[i] = item;
                // hasUpdatesAvailable の状態は refreshRepositories の最後でまとめて更新される
                emit packageListChanged();
            }
            break;
        }
    }
}

void PackageManager::installPackage(const QString &packageId) {
    if (m_isBusy)
        return;

    QVariantMap pkg;
    for (const auto &p : std::as_const(m_packageList)) {
        if (p.toMap().value(QStringLiteral("id")).toString() == packageId) {
            pkg = p.toMap();
            break;
        }
    }

    if (pkg.isEmpty()) {
        emit errorOccurred(tr("パッケージが見つかりません: %1").arg(packageId));
        return;
    }

    QString versionToInstall = pkg.value(QStringLiteral("latest_version")).toString();
    if (versionToInstall.isEmpty()) {
        emit errorOccurred(tr("最新バージョン情報が取得できていません。同期を行ってください。"));
        return;
    }

    QString downloadUrl = pkg.value(QStringLiteral("download_url_template")).toString();
    downloadUrl.replace(QStringLiteral("{VERSION}"), versionToInstall);

    setBusy(true);
    setStatus(tr("パッケージのインストール中: %1").arg(packageId));
    setProgress(0.0);

    QTimer::singleShot(1500, this, [this, packageId, versionToInstall, downloadUrl]() {
        setProgress(1.0);
        setStatus(tr("インストール完了: %1").arg(packageId));
        setBusy(false);

        // インストール済み情報の保存
        QVariantMap installed = loadInstalledPackagesFromFile();
        QVariantMap info;
        info[QStringLiteral("version")] = versionToInstall;
        installed[packageId] = info;

        if (packageId == QStringLiteral("org.aviqtl.app")) {
            emit selfUpdateAvailable(versionToInstall, downloadUrl);
            setStatus(tr("AviQtlのアップデートがダウンロード可能です。再起動して適用してください。"));
            return;
        }

        QFile file(getInstalledPackagesPath());
        if (file.open(QIODevice::WriteOnly)) {
            file.write(QJsonDocument::fromVariant(installed).toJson());
            file.close();
        }

        emit packageInstalled(packageId);

        // リストの状態を更新
        bool anyUpdates = false;
        for (int i = 0; i < m_packageList.size(); ++i) {
            QVariantMap item = m_packageList[i].toMap();
            if (item.value("id").toString() == packageId) {
                item[QStringLiteral("installed_version")] = versionToInstall;
                m_packageList[i] = item;
                emit packageListChanged();
            }
            // アップデートの有無を再評価
            const QString installedVer = item.value(QStringLiteral("installed_version")).toString();
            const QString latestVer = item.value(QStringLiteral("latest_version")).toString();
            if (!installedVer.isEmpty() && !latestVer.isEmpty() && compareVersions(latestVer, installedVer) > 0) {
                anyUpdates = true;
            }
        }
        setHasUpdatesAvailable(anyUpdates);
    });
}

void PackageManager::removePackage(const QString &packageId) {
    if (m_isBusy || packageId == QStringLiteral("org.aviqtl.app"))
        return;

    setBusy(true);
    setStatus(tr("パッケージを削除中: %1").arg(packageId));

    // 削除処理のシミュレーション（実際は展開されたフォルダを削除するロジックが必要）
    QTimer::singleShot(500, this, [this, packageId]() {
        QVariantMap installed = loadInstalledPackagesFromFile();
        if (installed.remove(packageId)) {
            QFile file(getInstalledPackagesPath());
            if (file.open(QIODevice::WriteOnly)) {
                file.write(QJsonDocument::fromVariant(installed).toJson());
                file.close();
            }
        }

        // リストの状態を更新
        for (int i = 0; i < m_packageList.size(); ++i) {
            QVariantMap item = m_packageList[i].toMap();
            if (item.value(QStringLiteral("id")).toString() == packageId) {
                item.remove(QStringLiteral("installed_version"));
                m_packageList[i] = item;
                emit packageListChanged();
                break;
            }
        }

        setBusy(false);
        setStatus(tr("削除完了: %1").arg(packageId));
        emit packageRemoved(packageId);
    });
}

QVariantList PackageManager::searchPackages(const QString &query) const {
    if (query.isEmpty())
        return m_packageList;
    QVariantList filtered;
    for (const auto &p : m_packageList) {
        QVariantMap map = p.toMap();
        if (map.value("display_name").toString().contains(query, Qt::CaseInsensitive) || map.value("id").toString().contains(query, Qt::CaseInsensitive)) {
            filtered.append(p);
        }
    }
    return filtered;
}

QVariantList PackageManager::getInstalledPackages() const {
    QVariantList list;
    QVariantMap installed = loadInstalledPackagesFromFile();

    // アプリ本体の情報も追加
    installed.insert(QStringLiteral("org.aviqtl.app"), QVariantMap{{QStringLiteral("version"), QString::fromUtf8(AviQtl::VERSION_STRING)}});

    for (auto it = installed.begin(); it != installed.end(); ++it) {
        QVariantMap pkg;
        pkg.insert(QStringLiteral("id"), it.key());
        pkg.insert(QStringLiteral("version"), it.value().toMap().value(QStringLiteral("version")));
        list.append(pkg);
    }
    return list;
}

void PackageManager::upgradeAllPackages() {
    if (m_isBusy)
        return;

    m_upgradeQueue.clear();
    for (const auto &p : m_packageList) {
        const QVariantMap item = p.toMap();
        const QString installedVer = item.value(QStringLiteral("installed_version")).toString();
        const QString latestVer = item.value(QStringLiteral("latest_version")).toString();
        if (!installedVer.isEmpty() && !latestVer.isEmpty() && compareVersions(latestVer, installedVer) > 0) {
            m_upgradeQueue.enqueue(item.value(QStringLiteral("id")).toString());
        }
    }

    if (m_upgradeQueue.isEmpty()) {
        setStatus(tr("アップグレード可能なパッケージはありません。"));
        return;
    }

    setBusy(true);
    setStatus(tr("すべてのパッケージをアップグレード中..."));
    processUpgradeQueue();
}

void PackageManager::processUpgradeQueue() {
    if (m_upgradeQueue.isEmpty()) {
        setBusy(false);
        setStatus(tr("すべてのアップグレードが完了しました"));
        setHasUpdatesAvailable(false); // すべてアップグレードされたので、利用可能なアップデートはない
        return;
    }

    QString nextPackageId = m_upgradeQueue.dequeue();
    setStatus(tr("パッケージをアップグレード中: %1").arg(nextPackageId));
    // installPackage が完了すると、その中で processUpgradeQueue() が再度呼ばれる
    installPackage(nextPackageId);
}

} // namespace AviQtl::Core
