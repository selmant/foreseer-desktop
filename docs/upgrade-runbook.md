# Jellium thin-fork upgrade runbook

Use this when rebasing the maintained thin fork onto newer upstream Jellium.

## Prerequisites

- Clean worktree for the thin fork (`main` branch).
- Foreseer Desktop worktree that will consume the new pin.
- Recorded upstream base in `jellium.upstream-base` and pin in `jellium.rev`.

## Steps

1. **Fetch upstream**
   ```sh
   git -C "$JELLIUM" fetch upstream
   git -C "$JELLIUM" checkout main
   ```
2. **Rebase thin branch**
   ```sh
   git -C "$JELLIUM" rebase upstream/main
   # resolve conflicts; keep the host-extension generic; drop product leakage
   ```
3. **Update recorded base** after a successful rebase onto the new tip:
   ```sh
   git -C "$JELLIUM" merge-base --is-ancestor "$(cat jellium.upstream-base)" HEAD
   printf '%s\n' "$(git -C "$JELLIUM" rev-parse upstream/main)" > jellium.upstream-base
   ```
   Prefer setting `jellium.upstream-base` to the upstream commit the thin branch now sits on (usually `upstream/main` at rebase time).
4. **Refresh patch manifest**
   - List `git log --oneline $(cat jellium.upstream-base)..HEAD`
   - Update `docs/jellium-patch-manifest.md`
   - Run `scripts/patch-delta.sh docs/jellium-patch-delta.md`
5. **Boundary audit**
   ```sh
   JELLIUM_DIR="$JELLIUM" ./scripts/boundary-audit.sh
   ```
6. **Stock + feature tests (Jellium)**
   ```sh
   cargo test -p jfn-cef
   cargo test -p jfn-rust
   cargo test -p jfn-cef --features host-extension
   cargo test -p jfn-rust --features host-extension
   ```
7. **Foreseer tests**
   ```sh
   cargo fmt -- --check
   cargo test
   cargo clippy --all-targets -- -D warnings
   node scripts/protocol-v1-harness.mjs
   JELLIUM_DIR="$JELLIUM" ./scripts/boundary-audit.sh
   ```
8. **Linux matrix (manual)** — Wayland and X11 smoke from `docs/migration-v2.md` Phase 7 checklist.
9. **Update pin**
   ```sh
   git -C "$JELLIUM" rev-parse HEAD > jellium.rev
   ```
10. **Commit** Foreseer pin + manifest + delta docs together.

## Failure modes

- Boundary audit leakage → move product strings/assets into Foreseer; never “allowlist” product names in Jellium.
- Unapproved commits → either drop them or document them in the patch manifest with rationale.
- Stock regression → fix in the thin fork before bumping `jellium.rev`.
