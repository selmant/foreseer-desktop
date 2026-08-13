# Protocol v1 / host-extension migration

## Pins and baselines

| Item | Value |
|------|-------|
| Protocol version | `1` |
| Old Foreseer baseline tag | `v0.2-baseline` (`5ce0e350319d6323c6d2ef47fad232fbe8842d36`) |
| Old Jellium release pin (`jellium.rev` at baseline) | `1242b0e6c48fc272cf1852b392501f75b71cd6d9` (tag `external-frontend-v1-archive`) |
| Old Jellium local hardened tip | preserved on `archive/local-runtime-fixes-20260812` |
| New upstream base (`upstream/main` at worktree create) | `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73` |
| Thin fork branch / checkout | `main` at `/home/selmant/Projects/jellium-desktop` |
| Thin fork tip | `ffa6e228e1f66e615f2a03e48b18c05bb27d6140` |
| Foreseer host-extension integration | released from `main` as Desktop `v0.2.8` |
| Foreseerr web contract | protocol v1 fixture on `develop` |

## Worktree rules

- Keep runtime experiments on their archive branch until they have passed the
  normal Jellium and Foreseer release gates.
- All host-extension maintenance happens in the canonical Desktop and Jellium
  `main` branches.

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
