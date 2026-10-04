#pragma once

#include <QString>

struct BatteryCapacity {
    qint64 full = -1;
    qint64 design = -1;
    bool energy = false;
    qint64 designVoltage = -1;

    double healthPercent() const {
        return full > 0 && design > 0 ? 100.0 * double(full) / double(design) : -1.0;
    }

    double wattHours(qint64 capacity) const {
        if (capacity <= 0) {
            return -1.0;
        }
        if (energy) {
            return double(capacity) / 1'000'000.0;
        }
        // sysfs reports charge in microamp-hours and voltage in microvolts.
        // Use design voltage so the estimate stays independent of charge level.
        return designVoltage > 0 ? double(capacity) * double(designVoltage) / 1'000'000'000'000.0 : -1.0;
    }
};

BatteryCapacity readBatteryCapacity(const QString &batteryPath);
