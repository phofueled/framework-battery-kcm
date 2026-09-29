#pragma once

#include <KQuickConfigModule>
#include <QDBusMessage>
#include <QTimer>
#include <QVariantList>

#include <functional>

class FrameworkBatteryKcm : public KQuickConfigModule {
    Q_OBJECT
    Q_PROPERTY(int chargePercent READ chargePercent NOTIFY statusChanged)
    Q_PROPERTY(QString batteryState READ batteryState NOTIFY statusChanged)
    Q_PROPERTY(int chargeLimit READ chargeLimit NOTIFY statusChanged)
    Q_PROPERTY(bool serviceAvailable READ serviceAvailable NOTIFY statusChanged)
    Q_PROPERTY(bool overrideAvailable READ overrideAvailable NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY statusChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY statusChanged)
    Q_PROPERTY(bool scheduleEnabled READ scheduleEnabled NOTIFY scheduleLoaded)
    Q_PROPERTY(int scheduleOutsideLimit READ scheduleOutsideLimit NOTIFY scheduleLoaded)
    Q_PROPERTY(bool scheduleHasLegacyEntries READ scheduleHasLegacyEntries NOTIFY scheduleLoaded)
    Q_PROPERTY(QVariantList scheduleEntries READ scheduleEntries NOTIFY scheduleLoaded)

public:
    FrameworkBatteryKcm(QObject *parent, const KPluginMetaData &data);

    int chargePercent() const { return m_chargePercent; }
    QString batteryState() const { return m_batteryState; }
    int chargeLimit() const { return m_chargeLimit; }
    bool serviceAvailable() const { return m_serviceAvailable; }
    bool overrideAvailable() const { return m_overrideAvailable; }
    bool busy() const { return m_busy; }
    QString lastError() const { return m_lastError; }
    bool scheduleEnabled() const { return m_scheduleEnabled; }
    int scheduleOutsideLimit() const { return m_scheduleOutsideLimit; }
    bool scheduleHasLegacyEntries() const { return m_scheduleHasLegacyEntries; }
    QVariantList scheduleEntries() const { return m_scheduleEntries; }

    Q_INVOKABLE void refresh();
    Q_INVOKABLE void setChargeLimit(int limit);
    Q_INVOKABLE void chargeToFullOnce();
    Q_INVOKABLE void saveSchedule(bool enabled, int outsideLimit, const QVariantList &entries);

Q_SIGNALS:
    void statusChanged();
    void scheduleLoaded();
    void operationSucceeded(const QString &message);

private:
    void refreshPower();
    void loadSchedule();
    void callWrite(const QString &method, const QList<QVariant> &arguments,
                   const std::function<void()> &onSuccess);
    void setError(const QString &message);
    static QDBusMessage request(const QString &method, const QList<QVariant> &arguments = {});

    QTimer m_refreshTimer;
    int m_chargePercent = -1;
    QString m_batteryState;
    int m_chargeLimit = -1;
    bool m_serviceAvailable = false;
    bool m_overrideAvailable = false;
    bool m_busy = false;
    QString m_lastError;
    bool m_scheduleEnabled = false;
    int m_scheduleOutsideLimit = 100;
    bool m_scheduleHasLegacyEntries = false;
    QVariantList m_scheduleEntries;
};
