#include "frameworkbatterykcm.hpp"

#include <KLocalizedString>
#include <KPluginFactory>
#include <QDBusConnection>
#include <QDBusError>
#include <QDBusPendingCallWatcher>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QVariantMap>

K_PLUGIN_CLASS_WITH_JSON(FrameworkBatteryKcm, "kcm_framework_battery.json")

namespace {
QString readText(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return {};
    }
    return QString::fromUtf8(file.readAll()).trimmed();
}
}

FrameworkBatteryKcm::FrameworkBatteryKcm(QObject *parent, const KPluginMetaData &data)
    : KQuickConfigModule(parent, data) {
    setButtons(NoAdditionalButton);
    m_refreshTimer.setInterval(30000);
    connect(&m_refreshTimer, &QTimer::timeout, this, &FrameworkBatteryKcm::refresh);
    m_refreshTimer.start();
    refresh();
    loadSchedule();
}

QDBusMessage FrameworkBatteryKcm::request(const QString &method, const QList<QVariant> &arguments) {
    auto message = QDBusMessage::createMethodCall(
        QStringLiteral("org.frameworkbattery.Control1"),
        QStringLiteral("/org/frameworkbattery/Control1"),
        QStringLiteral("org.frameworkbattery.Control1"), method);
    message.setArguments(arguments);
    return message;
}

void FrameworkBatteryKcm::setError(const QString &message) {
    m_lastError = message;
    Q_EMIT statusChanged();
}

void FrameworkBatteryKcm::refreshPower() {
    m_chargePercent = -1;
    m_batteryState = i18n("Battery unavailable");
    const QDir supplies(QStringLiteral("/sys/class/power_supply"));
    for (const auto &entry : supplies.entryList(QDir::Dirs | QDir::NoDotAndDotDot | QDir::System)) {
        const auto base = supplies.filePath(entry);
        if (readText(base + QStringLiteral("/type")) != QStringLiteral("Battery")) {
            continue;
        }
        bool ok = false;
        const int percent = readText(base + QStringLiteral("/capacity")).toInt(&ok);
        if (ok && percent >= 0 && percent <= 100) {
            m_chargePercent = percent;
        }
        m_batteryState = readText(base + QStringLiteral("/status"));
        if (m_batteryState.isEmpty()) {
            m_batteryState = i18n("Unknown state");
        }
        break;
    }
    Q_EMIT statusChanged();
}

void FrameworkBatteryKcm::refresh() {
    refreshPower();
    auto *watcher = new QDBusPendingCallWatcher(
        QDBusConnection::systemBus().asyncCall(request(QStringLiteral("GetChargeLimit"))), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, watcher] {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        if (reply.type() == QDBusMessage::ErrorMessage) {
            m_serviceAvailable = false;
            m_chargeLimit = -1;
            setError(reply.errorMessage());
            return;
        }
        m_serviceAvailable = true;
        m_chargeLimit = reply.arguments().value(0).toInt();
        m_lastError.clear();
        Q_EMIT statusChanged();
    });
    auto *overrideWatcher = new QDBusPendingCallWatcher(
        QDBusConnection::systemBus().asyncCall(request(QStringLiteral("GetOverrideAvailable"))), this);
    connect(overrideWatcher, &QDBusPendingCallWatcher::finished, this, [this, overrideWatcher] {
        const auto reply = overrideWatcher->reply();
        overrideWatcher->deleteLater();
        m_overrideAvailable = reply.type() != QDBusMessage::ErrorMessage && reply.arguments().value(0).toBool();
        Q_EMIT statusChanged();
    });
}

void FrameworkBatteryKcm::loadSchedule() {
    auto *watcher = new QDBusPendingCallWatcher(
        QDBusConnection::systemBus().asyncCall(request(QStringLiteral("GetSchedule"))), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, watcher] {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        if (reply.type() == QDBusMessage::ErrorMessage) {
            setError(reply.errorMessage());
            return;
        }
        const auto document = QJsonDocument::fromJson(reply.arguments().value(0).toString().toUtf8());
        if (!document.isObject()) {
            setError(i18n("The saved schedule could not be read."));
            return;
        }
        const auto object = document.object();
        m_scheduleEnabled = object.value(QStringLiteral("enabled")).toBool();
        m_scheduleOutsideLimit = object.value(QStringLiteral("outside_limit")).toInt(100);
        m_scheduleEntries = object.value(QStringLiteral("entries")).toArray().toVariantList();
        m_scheduleHasLegacyEntries = false;
        for (const auto &entry : object.value(QStringLiteral("entries")).toArray()) {
            if (!entry.toObject().value(QStringLiteral("end_minute")).isDouble()) {
                m_scheduleHasLegacyEntries = true;
                break;
            }
        }
        Q_EMIT scheduleLoaded();
    });
}

void FrameworkBatteryKcm::callWrite(const QString &method, const QList<QVariant> &arguments,
                                    const std::function<void()> &onSuccess) {
    if (m_busy) {
        return;
    }
    m_busy = true;
    m_lastError.clear();
    Q_EMIT statusChanged();
    auto *watcher = new QDBusPendingCallWatcher(
        QDBusConnection::systemBus().asyncCall(request(method, arguments)), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, watcher, onSuccess] {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        m_busy = false;
        if (reply.type() == QDBusMessage::ErrorMessage) {
            setError(reply.errorMessage());
            return;
        }
        Q_EMIT statusChanged();
        onSuccess();
    });
}

void FrameworkBatteryKcm::setChargeLimit(int limit) {
    if (limit < 25 || limit > 100) {
        setError(i18n("Charge limit must be between 25% and 100%."));
        return;
    }
    callWrite(QStringLiteral("SetChargeLimit"), {QVariant::fromValue(uint(limit))}, [this] {
        Q_EMIT operationSucceeded(i18n("Charge limit updated."));
        refresh();
    });
}

void FrameworkBatteryKcm::chargeToFullOnce() {
    callWrite(QStringLiteral("ChargeToFullOnce"), {}, [this] {
        Q_EMIT operationSucceeded(i18n("One-time full charge requested."));
        refresh();
    });
}

void FrameworkBatteryKcm::saveSchedule(bool enabled, int outsideLimit, const QVariantList &entries) {
    if (outsideLimit < 25 || outsideLimit > 100) {
        setError(i18n("Outside-hours limit must be between 25% and 100%."));
        return;
    }
    QVariantMap value;
    value.insert(QStringLiteral("enabled"), enabled);
    value.insert(QStringLiteral("outside_limit"), outsideLimit);
    value.insert(QStringLiteral("entries"), entries);
    const auto json = QString::fromUtf8(QJsonDocument::fromVariant(value).toJson(QJsonDocument::Compact));
    callWrite(QStringLiteral("SetSchedule"), {json}, [this] {
        Q_EMIT operationSucceeded(i18n("Schedule saved."));
        loadSchedule();
        refresh();
    });
}

#include "frameworkbatterykcm.moc"
