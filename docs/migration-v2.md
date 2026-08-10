# Protocol v2 / host-extension migration

## Pins and baselines

| Item | Value |
|------|-------|
| Protocol version | `2` |
| Old Foreseer baseline tag | `v0.2-baseline` (`5ce0e350319d6323c6d2ef47fad232fbe8842d36`) |
| Old Jellium release pin (`jellium.rev` at baseline) | `1242b0e6c48fc272cf1852b392501f75b71cd6d9` (tag `external-frontend-v1-archive`) |
| Old Jellium local hardened tip (dirty worktree left alone) | `8714375c676fd2ec771dc1471f954f409ef7b001` (tag `external-frontend-v1-local-hardened`) |
| New upstream base (`upstream/main` at worktree create) | `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73` |
| Thin fork branch / worktree | `host-extension` at `/home/selmant/Projects/jellium-desktop-host-ext` |
| Thin fork tip | `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73` (update as generic commits land) |
| Foreseer v2 branch / worktree | `feat/host-extension-v2` at `/home/selmant/Projects/foreseer-desktop-v2` |
| Seerr v2 branch | `feat/foreseer-native-v2` from `develop` |

## Worktree rules

- Do not modify the dirty original Jellium checkout (`/home/selmant/Projects/jellium-desktop`) or its five uncommitted files.
- Do not overwrite the dirty `Cargo.lock` in `/home/selmant/Projects/foreseer-desktop`.
- All v2 / host-extension work happens in the worktrees and branches listed above.

## Status

- [x] Phase 0 baselines and worktrees
- [ ] Phase 1 generic `host-extension` seam
- [ ] Phase 2 fork triage
- [ ] Phase 3 Foreseer protocol + controller
- [ ] Phase 4 assets + live adapter
- [ ] Phase 5 Seerr v2
- [ ] Phase 6 gates + docs
- [ ] Phase 7 Linux acceptance + cutover
