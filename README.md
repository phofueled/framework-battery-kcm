# Framework Battery for KDE

A small KDE System Settings pane for Framework laptop battery controls on CachyOS and Arch Linux. The pane is Qt/QML with Kirigami; a Rust system D-Bus service uses Framework's `framework_lib` for embedded-controller operations. It does not depend on `framework_tool` or copy code from `framework-kcm`.

The first version shows battery percentage and charging state, reads and sets the 25–100% charge limit, and manages weekly charge-limit entries. A one-time full charge uses the EC override command. The service runs only while a KCM request is active, then exits after 60 seconds of inactivity. The schedule uses a systemd timer; no scheduler stays resident.

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

Open **Framework Battery** in System Settings, or run `kcmshell6 kcm_framework_battery`. Installing the package registers its system D-Bus service and polkit action. A charge-limit write prompts for administrator authentication. A schedule change writes root-owned configuration and enables or disables `framework-battery-schedule.timer`; disabling it leaves the current EC limit alone.

Each entry selects weekdays, a local time, and a limit. On boot or when a schedule is saved, the most recent weekly entry is applied. Manual changes last until the next schedule event. The next event can also take precedence over a one-time full-charge request.

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
qmllint kcm/ui/main.qml kcm/ui/SchedulePage.qml
systemd-analyze calendar 'Mon *-*-* 08:00:00'
```

The helper deliberately requires `/dev/cros_ec` and a Framework DMI vendor. It does not use Framework System's raw port-I/O fallback. On unsupported hardware or where the EC driver is missing, the pane shows an error and disables the controls.

`framework_lib` is BSD-3-Clause. This project uses the battery-page and KCM integration ideas from `framework-kcm`, but its source code and UI were written independently.
