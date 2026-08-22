#!/usr/bin/env bash
# Stage a target-native, production-only Foreseerr bundle for a desktop build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${FORESEERR_DIR:-$ROOT/../SeerrSuggestArr}"
NODE_BIN="${FORESEERR_NODE_BIN:-$(command -v node)}"
DEST="${1:-$ROOT/resources}"
VERSION_PIN="$(tr -d '[:space:]' < "$ROOT/foreseerr.version")"
NODE_PIN="$(tr -d '[:space:]' < "$ROOT/node.rev")"

if [[ ! -x "$NODE_BIN" ]]; then
  echo "stage-foreseerr: provide FORESEERR_NODE_BIN or install Node 22" >&2
  exit 1
fi
if [[ "$($NODE_BIN --version)" != "$NODE_PIN" ]]; then
  echo "stage-foreseerr: Node $NODE_PIN is required" >&2
  exit 1
fi
if [[ ! -f "$SOURCE/package.json" ]]; then
  echo "stage-foreseerr: no Foreseerr checkout at $SOURCE" >&2
  exit 1
fi
VERSION="$($NODE_BIN -p "require('$SOURCE/package.json').version")"
if [[ "$VERSION" != "$VERSION_PIN" ]]; then
  echo "stage-foreseerr: Foreseerr $VERSION does not match foreseerr.version $VERSION_PIN" >&2
  exit 1
fi

pnpm --dir "$SOURCE" build
rm -rf "$DEST/foreseerr" "$DEST/node"
mkdir -p "$DEST/foreseerr" "$DEST/node"
install -m 0755 "$NODE_BIN" "$DEST/node/node"
# `deploy --prod` gives the managed server an isolated, target-native production
# dependency tree. Do not copy the development checkout's node_modules: it
# contains Cypress, compiler tooling, package-manager stores, and host-native
# modules which are unsafe to ship in a release artifact.
pnpm --dir "$SOURCE" --filter foreseerr --prod deploy --legacy "$DEST/foreseerr"
for item in launcher.js dist .next public seerr-api.yml; do
  [[ -e "$SOURCE/$item" ]] && cp -a "$SOURCE/$item" "$DEST/foreseerr/"
done
find "$DEST/foreseerr" -type d \( -name '.cache' -o -name 'cypress' -o -name 'test' -o -name 'tests' \) -prune -exec rm -rf {} +
find "$DEST/foreseerr" -type f \( -name '*.map' -o -name '*.ts' -o -name '*.tsx' \) -delete

# The official Node distribution places its license beside bin/node. Preserve
# it with deterministic notices for every deployed production dependency.
NODE_LICENSE="$(dirname "$NODE_BIN")/../LICENSE"
if [[ -f "$NODE_LICENSE" ]]; then
  install -m 0644 "$NODE_LICENSE" "$DEST/node/LICENSE"
fi
"$NODE_BIN" "$ROOT/scripts/generate-third-party-notices.mjs" \
  "$DEST/foreseerr" "$NODE_PIN" "$DEST/THIRD_PARTY_NOTICES.txt"
test -x "$DEST/node/node"
test -f "$DEST/foreseerr/launcher.js"
test -d "$DEST/foreseerr/dist"
test -f "$DEST/THIRD_PARTY_NOTICES.txt"
echo "stage-foreseerr: staged $VERSION_PIN in $DEST"
