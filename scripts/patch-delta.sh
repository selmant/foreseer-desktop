#!/usr/bin/env bash
# Print the thin-fork patch delta between the recorded upstream base and the pin.
# Growth beyond the documented stack requires updating the patch manifest.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN="$(tr -d '[:space:]' < "$ROOT/jellium.rev")"
UPSTREAM_BASE="$(tr -d '[:space:]' < "$ROOT/jellium.upstream-base")"

resolve_jellium() {
  if [[ -n "${JELLIUM_DIR:-}" ]]; then
    printf '%s\n' "$JELLIUM_DIR"
    return
  fi
  for candidate in \
    "$ROOT/../jellium-desktop" \
    "$ROOT/../jellium-desktop-host-ext"; do
    if [[ -d "$candidate/.git" || -f "$candidate/.git" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  echo "patch-delta: no Jellium checkout found (set JELLIUM_DIR)" >&2
  exit 1
}

JELLIUM="$(cd "$(resolve_jellium)" && pwd)"
OUT="${1:-}"

{
  echo "# Jellium thin-fork patch delta"
  echo
  echo "- upstream base: \`$UPSTREAM_BASE\`"
  echo "- pin / HEAD: \`$PIN\`"
  echo "- checkout: \`$JELLIUM\`"
  echo
  echo "## Commits"
  echo
  git -C "$JELLIUM" log --oneline "${UPSTREAM_BASE}..${PIN}"
  echo
  echo "## Diffstat"
  echo
  echo '```'
  git -C "$JELLIUM" diff --stat "${UPSTREAM_BASE}..${PIN}"
  echo '```'
  echo
  echo "## File list"
  echo
  git -C "$JELLIUM" diff --name-only "${UPSTREAM_BASE}..${PIN}" | sed 's/^/- /'
} | if [[ -n "$OUT" ]]; then
  tee "$OUT"
else
  cat
fi
