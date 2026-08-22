#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORESEERR_DIR="${FORESEERR_DIR:-$ROOT/../SeerrSuggestArr}"
test -s "$ROOT/jellium.rev"
test -s "$ROOT/foreseerr.rev"
test -f "$FORESEERR_DIR/package.json"
PIN="$(tr -d '[:space:]' < "$ROOT/foreseerr.rev")"
VERSION="$(node -p "require('$FORESEERR_DIR/package.json').version")"
test "$PIN" = "$VERSION"
test -f "$FORESEERR_DIR/launcher.js"
echo "release pins: Foreseer Desktop $(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1), Foreseerr $PIN"
