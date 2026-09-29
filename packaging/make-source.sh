#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
version=0.2.2
archive="framework-battery-${version}.tar.gz"
tar -C .. \
    --exclude='./target' \
    --exclude='./build' \
    --exclude='./.git' \
    --exclude='./packaging/src' \
    --exclude='./packaging/pkg' \
    --exclude='./packaging/*.tar.gz' \
    --exclude='./packaging/*.pkg.tar.zst' \
    --exclude='./packaging/.SRCINFO' \
    --transform="s,^\.,framework-battery-${version}," \
    -czf "$archive" .
printf 'Created %s\n' "$archive"
