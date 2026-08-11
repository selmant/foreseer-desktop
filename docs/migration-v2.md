# Protocol v1 / host-extension migration

## Pins and baselines

| Item | Value |
|------|-------|
| Protocol version | `2` |
| Old Foreseer baseline tag | `v0.2-baseline` (`5ce0e350319d6323c6d2ef47fad232fbe8842d36`) |
| Old Jellium release pin (`jellium.rev` at baseline) | `1242b0e6c48fc272cf1852b392501f75b71cd6d9` (tag `external-frontend-v1-archive`) |
| Old Jellium local hardened tip (dirty worktree left alone) | `8714375c676fd2ec771dc1471f954f409ef7b001` (tag `external-frontend-v1-local-hardened`) |
| New upstream base (`upstream/main` at worktree create) | `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73` |
| Thin fork branch / worktree | `host-extension` at `/home/selmant/Projects/jellium-desktop-host-ext` |
| Thin fork tip | `ce5d4b5ce4952634fc7cabb18f7cf0a00b5e21b4` |
| Foreseer v2 branch / worktree | `feat/host-extension-v2` at `/home/selmant/Projects/foreseer-desktop-v2` |
| Seerr v2 branch | `feat/foreseer-native-v2` from `develop` |

## Worktree rules

- Do not modify the dirty original Jellium checkout (`/home/selmant/Projects/jellium-desktop`) or its five uncommitted files.
- Do not overwrite the dirty `Cargo.lock` in `/home/selmant/Projects/foreseer-desktop`.
- All v2 / host-extension work happens in the worktrees and branches listed above.

## Status

- [x] Phase 0 baselines and worktrees
- [x] Phase 1 generic `host-extension` seam
- [x] Phase 2 fork triage
- [x] Phase 3 Foreseer protocol + controller
- [x] Phase 4 assets + live adapter
- [x] Phase 5 Seerr v2 (code complete; deploy before desktop release)
- [x] Phase 6 gates + docs (upstream PRs deferred / non-blocking)
- [ ] Phase 7 Linux acceptance + cutover
  - [x] 7.1 Automated gates (local): Jellium stock + `host-extension`; Foreseer fmt/test/clippy/harness/boundary-audit; Seerr `pnpm test`
  - [ ] 7.2 Manual Linux matrix + 50-cycle soak
  - [ ] 7.3 Cutover (`jellium.rev` already candidate-pinned; push thin fork + deploy Seerr first)
  - [ ] 7.4 Completion check

## Upstream PR drafts (Phase 6.4, non-blocking)

Open against upstream Jellium without Foreseer names/endpoints in titles or bodies:

1. Wayland full-buffer viewport during WSI resize (`de3c381`)
2. mpv-proxy protocol error logging (`0a12974`)
3. Generic host-extension seam / structured transport (`c9e8deb`) — after 1–2 land or as follow-up


## Triage notes (Phase 2)

- Dropped: CEF severity mapping (already on upstream `28f2cf1`).
- Ported: Wayland full-buffer viewport during WSI resize; mpv-proxy protocol error logging.
- Skipped for Jellium (move to Foreseer private-web asset): dirty `input-plugin.js` resume generation/getItem behavior.
- Root-window diagnostics: upstream calloop path already logs dispatch/source failures; dirty hunk not ported.
- Left behind: old `external-frontend` product protocol, `external-host.js`, protocol fixtures, auth/config traits.
