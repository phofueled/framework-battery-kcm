# Framework Battery for KDE

A small KDE System Settings pane for Framework laptop battery controls on CachyOS and Arch Linux. The pane is Qt/QML with Kirigami; a Rust system D-Bus service sends only the required charge commands through Linux `/dev/cros_ec`. Its command definitions follow Framework System's BSD-licensed EC protocol. It does not depend on `framework_tool` or copy code from `framework-kcm`.

The pane shows battery percentage, charging state, cycle count, estimated battery health, full charge capacity, and design capacity. It reads and sets the 25–100% charge limit and manages weekly charge-limit profiles. A one-time full charge uses the EC override command. The service starts on demand and exits after 15 seconds of inactivity. The schedule uses a systemd timer; no scheduler stays resident.

## Build and install on CachyOS or Arch

Install build dependencies with pacman:

```sh
sudo pacman -S --needed base-devel cargo cmake extra-cmake-modules kcmutils kirigami polkit qt6-declarative
```

Build a local package from the current source tree:

```sh
cd packaging
./make-source.sh
makepkg -si
```

Open **Framework Battery** in System Settings, or run `kcmshell6 kcm_framework_battery`. Installing the package registers its system D-Bus service and polkit action. By default, writes prompt for administrator authentication. A schedule change writes root-owned configuration and enables or disables `framework-battery-schedule.timer`; disabling it leaves the current EC limit alone.

### Prompt-free changes for one local administrator

The pane does not call `sudo`. Its D-Bus service runs as root, and polkit authorizes each write. On a personal machine, run this one-time setup to allow a named `wheel` user to change Framework battery settings without a password prompt:

```sh
sudo ./packaging/enable-passwordless.sh "$USER"
```

The rule applies only to the named user in an active local session and only to `org.frameworkbattery.modify`. Other users keep the normal polkit policy. Remove `/etc/polkit-1/rules.d/49-framework-battery-USERNAME.rules` as root to revoke access.

Each profile selects start weekdays, 24-hour start and end times, and a charge limit. A single range slider adjusts the window in 15-minute steps; the time fields allow exact minutes. An end time earlier than the start time means the next day. Profiles can be enabled individually, but enabled windows cannot overlap. A separate 25–100% limit applies outside active windows. On boot or when a schedule is saved, the limit for the current window is applied. Manual changes last until the next start or end event. The next event can also take precedence over a one-time full-charge request.

Version 1 event-based schedules continue to run unchanged until a new profile schedule is saved. The editor shows a notice when it loads an older schedule and converts the displayed entries to windows when saved.

The one-time button starts disabled. To verify the EC override on a device, run:

```sh
sudo /usr/lib/framework-battery/framework-battery-helper verify-override
```

**This sends a one-time full-charge request to the EC.** The helper checks that the saved limit did not change, then enables the button for subsequent use. Close and reopen the pane, or press Refresh. Do this only when a one-time full charge is wanted.

For a complete hardware check, `sudo /usr/lib/framework-battery/framework-battery-helper self-test` reads the current limit, changes it by 1%, restores it, and performs the same one-time override verification. This also requests a one-time full charge.

## Development checks

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build
ctest --test-dir build --output-on-failure
qmllint kcm/ui/main.qml kcm/ui/SchedulePage.qml
systemd-analyze calendar 'Mon *-*-* 08:00:00'
```

The helper deliberately requires `/dev/cros_ec` and a Framework DMI vendor. It does not use Framework System's raw port-I/O fallback. On unsupported hardware or where the EC driver is missing, the pane shows an error and disables the controls.

Battery health is the last full charge capacity divided by design capacity. Capacities come from Linux power-supply data and are displayed in Wh. When only charge capacity is available, the pane converts it using the reported design voltage, rather than the fluctuating present voltage. Health uses one matching pair of charge or energy readings. The cycle count is read as a 32-bit value from the EC, matching Framework Tool; some Framework firmware exposes a truncated count through ACPI. These readings add no runtime dependencies.

Release builds favor size (`opt-level = "z"` and full LTO). This keeps the existing D-Bus, polkit, atomic schedule writes, and local-time scheduling code while reducing the installed helper footprint.

The EC command definitions and ioctl layout are based on BSD-3-Clause licensed Framework System `framework_lib` 0.6.6 and the Linux `cros_ec_dev` interface. This project uses the battery-page and KCM integration ideas from `framework-kcm`, but its source code and UI were written independently.
