#include "batterycapacity.hpp"

#include <QFile>
#include <QTemporaryDir>

#include <cmath>
#include <iostream>

int main() {
    QTemporaryDir directory;
    if (!directory.isValid()) {
        return 1;
    }
    const auto write = [&directory](const char *name, const char *value) {
        QFile file(directory.filePath(QString::fromLatin1(name)));
        return file.open(QIODevice::WriteOnly) && file.write(value) == qint64(qstrlen(value));
    };
    const auto check = [](bool condition, const char *message) {
        if (!condition) {
            std::cerr << message << '\n';
        }
        return condition;
    };
    bool passed = true;
    auto capacity = readBatteryCapacity(directory.path());
    passed &= check(capacity.full == -1 && capacity.design == -1 && capacity.healthPercent() == -1,
                    "Missing capacities must be unavailable");
    if (!write("charge_full", "3072000\n") || !write("charge_full_design", "3572000\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(!capacity.energy && std::abs(capacity.healthPercent() - 86.00224) < 0.001,
                    "Health must compare full capacity with design, independently of charge level");
    passed &= check(capacity.wattHours(capacity.full) == -1,
                    "Charge capacity cannot be converted without a design voltage");
    if (!write("voltage_min_design", "15400000\n") || !write("voltage_now", "17000000\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(std::abs(capacity.wattHours(capacity.full) - 47.3088) < 0.0001
                        && std::abs(capacity.wattHours(capacity.design) - 55.0088) < 0.0001,
                    "Charge capacities must use design voltage to convert to Wh");
    if (!write("voltage_now", "14500000\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(std::abs(capacity.wattHours(capacity.full) - 47.3088) < 0.0001,
                    "Reported capacity must not fluctuate with the present voltage");
    if (!write("energy_full", "45000000\n") || !write("energy_full_design", "50000000\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(capacity.energy && capacity.wattHours(capacity.full) == 45
                        && capacity.wattHours(capacity.design) == 50 && capacity.healthPercent() == 90,
                    "A complete energy pair should use Wh");
    if (!write("energy_full_design", "invalid\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(!capacity.energy && capacity.design == 3572000,
                    "An incomplete energy pair must fall back to a complete charge pair");
    QFile::remove(directory.filePath(QStringLiteral("charge_full")));
    capacity = readBatteryCapacity(directory.path());
    passed &= check(capacity.energy && capacity.full == 45000000 && capacity.design == -1
                        && capacity.healthPercent() == -1,
                    "Energy and charge must never be mixed to calculate health");
    QFile::remove(directory.filePath(QStringLiteral("energy_full")));
    if (!write("charge_full", "0\n")) {
        return 1;
    }
    capacity = readBatteryCapacity(directory.path());
    passed &= check(capacity.full == -1 && capacity.healthPercent() == -1,
                    "Zero capacity must not produce a misleading health percentage");
    return passed ? 0 : 1;
}
