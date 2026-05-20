#pragma once
#include <QObject>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>

namespace AviQtl::Core {

class PackageManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool isBusy READ isBusy NOTIFY isBusyChanged)
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusTextChanged)
    Q_PROPERTY(double progress READ progress NOTIFY progressChanged)
    Q_PROPERTY(QStringList repositories READ repositories NOTIFY repositoriesChanged)

  public:
    static PackageManager &instance();

    bool isBusy() const { return m_isBusy; }
    QString statusText() const { return m_statusText; }
    double progress() const { return m_progress; }
    QStringList repositories() const;

    Q_INVOKABLE void refreshRepositories();
    Q_INVOKABLE void addRepository(const QString &url);
    Q_INVOKABLE void removeRepository(const QString &url);
    Q_INVOKABLE void installPackage(const QString &packageId);
    Q_INVOKABLE void removePackage(const QString &packageId);
    Q_INVOKABLE QVariantList searchPackages(const QString &query) const;
    Q_INVOKABLE QVariantList getInstalledPackages() const;

  signals:
    void isBusyChanged();
    void statusTextChanged();
    void progressChanged();
    void repositoryRefreshed();
    void repositoriesChanged();
    void packageInstalled(const QString &packageId);
    void packageRemoved(const QString &packageId);
    void errorOccurred(const QString &message);

  private:
    explicit PackageManager(QObject *parent = nullptr);
    ~PackageManager() override = default;

    void setBusy(bool busy);
    void setStatus(const QString &status);
    void setProgress(double p);

    bool m_isBusy = false;
    QString m_statusText;
    double m_progress = 0.0;
};

} // namespace AviQtl::Core
