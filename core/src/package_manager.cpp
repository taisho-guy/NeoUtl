#include "package_manager.hpp"
#include "settings_manager.hpp"
#include <QCoreApplication>
#include <QDir>
#include <QTimer>

namespace AviQtl::Core {

PackageManager &PackageManager::instance() {
    static PackageManager instance;
    return instance;
}

PackageManager::PackageManager(QObject *parent) : QObject(parent) { m_statusText = tr("待機中"); }

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
    setStatus(tr("リポジトリを同期中..."));
    setProgress(0.0);

    // TODO: 登録されているリポジトリのインデックスJSON（最小構成）を取得し、パッケージリストを更新する
    QTimer::singleShot(1000, this, [this]() {
        setProgress(1.0);
        setStatus(tr("同期完了"));
        setBusy(false);
        emit repositoryRefreshed();
    });
}

void PackageManager::installPackage(const QString &packageId) {
    if (m_isBusy)
        return;
    setBusy(true);
    setStatus(tr("パッケージのインストール中: %1").arg(packageId));
    setProgress(0.0);

    // 内部ロジック案:
    // 1. リポジトリJSONから対象の metadata を取得
    // 2. metadata.release_feed (Atom/RSS) を QNetworkAccessManager で取得
    // 3. QXmlStreamReader でフィードを解析し、最新の <entry> からバージョン（tag等）を抽出
    // 4. download_url_template の {VERSION} を置換して実際の ZIP URL を構築
    // 5. ZIPをダウンロードし、type に応じたディレクトリ（effects/objects/plugins）へ展開
    // 6. ローカルの local.json にインストール済み ID とバージョンを記録

    QTimer::singleShot(1500, this, [this, packageId]() {
        // 仮のインストール先決定ロジック
        // QString targetDir;
        // if (type == "effect") targetDir = "effects";
        // else if (type == "mod") targetDir = "plugins";
        // ...

        setProgress(1.0);
        setStatus(tr("インストール完了: %1").arg(packageId));
        setBusy(false);
        emit packageInstalled(packageId);

        // インストール後に各エンジンの再ロードを促す必要があるかもしれません
        // 例: EffectRegistry::instance().loadEffectsFromDirectory(targetPath);
    });
}

void PackageManager::removePackage(const QString &packageId) {
    // TODO: ローカルファイル削除
}

QVariantList PackageManager::searchPackages(const QString &query) const {
    // TODO: インメモリのDBから検索
    return {};
}

QVariantList PackageManager::getInstalledPackages() const {
    // TODO: local.json からインストール済み一覧を返す
    return {};
}

} // namespace AviQtl::Core
