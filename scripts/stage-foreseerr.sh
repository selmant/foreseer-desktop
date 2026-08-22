#!/usr/bin/env bash
# Stage a target-native, production-only Foreseerr bundle for a desktop build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${FORESEERR_DIR:-$ROOT/../SeerrSuggestArr}"
NODE_BIN="${FORESEERR_NODE_BIN:-$(command -v node)}"
DEST="${1:-$ROOT/resources}"
PIN="$(tr -d '[:space:]' < "$ROOT/foreseerr.rev")"

if [[ ! -x "$NODE_BIN" ]]; then
  echo "stage-foreseerr: provide FORESEERR_NODE_BIN or install Node 22" >&2
  exit 1
fi
if [[ ! -f "$SOURCE/package.json" ]]; then
  echo "stage-foreseerr: no Foreseerr checkout at $SOURCE" >&2
  exit 1
fi
VERSION="$($NODE_BIN -p "require('$SOURCE/package.json').version")"
if [[ "$VERSION" != "$PIN" ]]; then
  echo "stage-foreseerr: Foreseerr $VERSION does not match foreseerr.rev $PIN" >&2
  exit 1
fi

pnpm --dir "$SOURCE" build
rm -rf "$DEST/foreseerr" "$DEST/node"
mkdir -p "$DEST/foreseerr" "$DEST/node"
install -m 0755 "$NODE_BIN" "$DEST/node/node"
install -m 0644 "$SOURCE/launcher.js" "$DEST/foreseerr/launcher.js"
for item in dist .next public node_modules seerr-api.yml; do
  [[ -e "$SOURCE/$item" ]] && cp -a "$SOURCE/$item" "$DEST/foreseerr/"
done
find "$DEST/foreseerr" -type d \( -name '.cache' -o -name 'cypress' -o -name 'test' -o -name 'tests' \) -prune -exec rm -rf {} +
find "$DEST/foreseerr" -type f \( -name '*.map' -o -name '*.ts' -o -name '*.tsx' \) -delete
test -x "$DEST/node/node"
test -f "$DEST/foreseerr/launcher.js"
test -d "$DEST/foreseerr/dist"
echo "stage-foreseerr: staged $PIN in $DEST"
