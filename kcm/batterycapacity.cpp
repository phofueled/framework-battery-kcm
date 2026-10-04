#include "batterycapacity.hpp"

#include <QFile>

namespace {
qint64 positiveNumber(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return -1;
    }
    bool ok = false;
    const auto value = file.readAll().trimmed().toLongLong(&ok);
    return ok && value > 0 ? value : -1;
}
}

BatteryCapacity readBatteryCapacity(const QString &batteryPath) {
    const BatteryCapacity energy{
        positiveNumber(batteryPath + QStringLiteral("/energy_full")),
        positiveNumber(batteryPath + QStringLiteral("/energy_full_design")), true};
    if (energy.full > 0 && energy.design > 0) {
        return energy;
    }
    const BatteryCapacity charge{
        positiveNumber(batteryPath + QStringLiteral("/charge_full")),
        positiveNumber(batteryPath + QStringLiteral("/charge_full_design")), false,
        positiveNumber(batteryPath + QStringLiteral("/voltage_min_design"))};
    // Use one complete pair when possible. Never compare charge with energy.
    if (charge.full > 0 && charge.design > 0) {
        return charge;
    }
    return energy.full > 0 || energy.design > 0 ? energy : charge;
}
