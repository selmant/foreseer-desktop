#!/usr/bin/env bash
# Fail if the pinned Jellium thin fork leaks Foreseer product surface, or if
# Foreseer imports anything beyond the public jfn-rust host-extension API.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIN="$(tr -d '[:space:]' < "$ROOT/jellium.rev")"
UPSTREAM_BASE="$(tr -d '[:space:]' < "$ROOT/jellium.upstream-base")"
MANIFEST="$ROOT/docs/jellium-patch-manifest.md"

resolve_jellium() {
  if [[ -n "${JELLIUM_DIR:-}" ]]; then
    printf '%s\n' "$JELLIUM_DIR"
    return
  fi
  for candidate in \
    "$ROOT/../jellium-desktop" \
    "$ROOT/../jellium-desktop-host-ext" \
    "$ROOT/../jellium-desktop-host-extension"; do
    if [[ -d "$candidate/.git" || -f "$candidate/.git" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  echo "boundary-audit: no Jellium checkout found (set JELLIUM_DIR)" >&2
  exit 1
}

JELLIUM="$(resolve_jellium)"
JELLIUM="$(cd "$JELLIUM" && pwd)"

echo "boundary-audit: foreseer=$ROOT"
echo "boundary-audit: jellium=$JELLIUM"
echo "boundary-audit: pin=$PIN"
echo "boundary-audit: upstream-base=$UPSTREAM_BASE"

HEAD="$(git -C "$JELLIUM" rev-parse HEAD)"
if [[ "$HEAD" != "$PIN" ]]; then
  echo "boundary-audit: Jellium HEAD $HEAD does not match jellium.rev $PIN" >&2
  exit 1
fi

if ! git -C "$JELLIUM" merge-base --is-ancestor "$UPSTREAM_BASE" HEAD; then
  echo "boundary-audit: upstream base $UPSTREAM_BASE is not an ancestor of $HEAD" >&2
  exit 1
fi

# --- product leakage into production Jellium sources/assets ---
EXCLUDE=(
  --glob '!**/.git/**'
  --glob '!**/target/**'
  --glob '!**/.cache/**'
  --glob '!**/node_modules/**'
  --glob '!**/*.md'
  --glob '!**/CHANGELOG*'
  --glob '!**/docs/jellium-patch-manifest.md'
)

LEAK_PATTERNS=(
  'Foreseer'
  'foreseer'
  'foreseerNative'
  'jelliumHost'
  'foreseer:native-event'
  'jellium:host-event'
  'protocol-v1\.json'
  'protocol-v2\.json'
  'external-host\.js'
  'foreseer-native\.js'
  'jellyfin-session\.js'
  'auth\.challenge'
  'auth\.complete'
  'play\.item'
  'session\.clear'
  '/api/v1/desktop/'
  'selmantrabzon\.com'
)

leak_hits=0
for pattern in "${LEAK_PATTERNS[@]}"; do
  if rg -n --hidden "${EXCLUDE[@]}" -e "$pattern" "$JELLIUM" >/tmp/boundary-audit-leak.txt 2>/dev/null; then
    echo "boundary-audit: leakage pattern /$pattern/:" >&2
    cat /tmp/boundary-audit-leak.txt >&2
    leak_hits=1
  fi
done
if [[ "$leak_hits" -ne 0 ]]; then
  echo "boundary-audit: product leakage detected in Jellium checkout" >&2
  exit 1
fi

# --- Foreseer may only import the public host-extension surface ---
ALLOWED_IMPORTS_RE='jfn_rust::(app::jfn_app_main_with|HostOptions|HostExtension|HostExtensionDescriptor|ExtensionSource|FrontendSource|Presentation|RuntimeEvent|RuntimeHandle|ExtensionConfigError|MAX_EXTENSION_PAYLOAD_BYTES)'
IMPORT_HITS="$(rg -n --glob 'src/**/*.rs' -e 'use jfn_rust::|jfn_rust::' "$ROOT" || true)"
if [[ -n "$IMPORT_HITS" ]]; then
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    code="${line#*:}"
    code="${code#*:}"
    # Allow `use jfn_rust::{ ... }` / path uses that only mention allowed symbols.
    if ! printf '%s\n' "$code" | rg -q 'jfn_rust::'; then
      continue
    fi
    # Strip brace imports into symbols for a coarse check.
    if printf '%s\n' "$code" | rg -q 'jfn_rust::\{'; then
      symbols="$(printf '%s\n' "$code" | sed -n 's/.*jfn_rust::{\([^}]*\)}.*/\1/p' | tr ',' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed 's/ as .*//')"
      while IFS= read -r sym; do
        [[ -z "$sym" ]] && continue
        case "$sym" in
          HostOptions|HostExtension|HostExtensionDescriptor|ExtensionSource|FrontendSource|Presentation|RuntimeEvent|RuntimeHandle|ExtensionConfigError|MAX_EXTENSION_PAYLOAD_BYTES|app) ;;
          *)
            echo "boundary-audit: disallowed jfn_rust import symbol '$sym' in $line" >&2
            exit 1
            ;;
        esac
      done <<< "$symbols"
      continue
    fi
    if ! printf '%s\n' "$code" | rg -q "$ALLOWED_IMPORTS_RE"; then
      echo "boundary-audit: disallowed jfn_rust path use: $line" >&2
      exit 1
    fi
  done <<< "$IMPORT_HITS"
fi

# --- commits on the pin must be listed in the patch manifest ---
if [[ ! -f "$MANIFEST" ]]; then
  echo "boundary-audit: missing $MANIFEST" >&2
  exit 1
fi

mapfile -t COMMITS < <(git -C "$JELLIUM" rev-list --reverse "${UPSTREAM_BASE}..${HEAD}")
for commit in "${COMMITS[@]}"; do
  short="${commit:0:7}"
  if ! rg -q --fixed-strings "$short" "$MANIFEST" && ! rg -q --fixed-strings "$commit" "$MANIFEST"; then
    subject="$(git -C "$JELLIUM" log -1 --pretty=%s "$commit")"
    echo "boundary-audit: unapproved thin-fork commit $short ($subject)" >&2
    echo "boundary-audit: add it to docs/jellium-patch-manifest.md or drop the commit" >&2
    exit 1
  fi
done

echo "boundary-audit: ok (${#COMMITS[@]} approved commits over upstream base)"
