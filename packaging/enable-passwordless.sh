#!/usr/bin/env bash
set -euo pipefail

if (( EUID != 0 )); then
    printf 'Run this setup script as root.\n' >&2
    exit 1
fi

username=${1:-}
if [[ ! $username =~ ^[a-z_][a-z0-9_-]*$ ]] || ! id "$username" >/dev/null 2>&1; then
    printf 'Provide an existing local username.\n' >&2
    exit 1
fi
if [[ " $(id -nG "$username") " != *' wheel '* ]]; then
    printf 'The selected user must belong to wheel.\n' >&2
    exit 1
fi

rules_dir=/etc/polkit-1/rules.d
install -d -m 0755 "$rules_dir"
temporary=$(mktemp "$rules_dir/.framework-battery.XXXXXX")
cat > "$temporary" <<EOF
polkit.addRule(function(action, subject) {
    if (action.id === "org.frameworkbattery.modify" &&
        subject.user === "$username" &&
        subject.active === true &&
        subject.local === true &&
        subject.isInGroup("wheel")) {
        return polkit.Result.YES;
    }
});
EOF
chmod 0644 "$temporary"
mv "$temporary" "$rules_dir/49-framework-battery-$username.rules"
printf 'Enabled prompt-free Framework battery changes for %s in an active local session.\n' "$username"
