# Jellium thin-fork patch manifest

Approved commits on top of `jellium.upstream-base`
(`28f2cf16a1f1b819884dd6a72919ca55bdf9bd73`). Any additional commit must be
recorded here before `scripts/boundary-audit.sh` will pass.

| Short | Full | Kind | Summary |
| --- | --- | --- | --- |
| `c9e8deb` | `c9e8deba673b8aa16b2424f8d7be157cb6d3aaae` | feature | Generic `host-extension` seam (descriptor, transport, presentation, example) |
| `de3c381` | `de3c381519c92706937f862145ae483c52d12348` | runtime fix | Wayland full-buffer viewport during WSI resize |
| `0a12974` | `0a1297427b07b4040f5cd58a14b7ad9a0b85750d` | runtime fix | mpv-proxy protocol error logging |
| `ce5d4b5` | `ce5d4b5ce4952634fc7cabb18f7cf0a00b5e21b4` | hygiene | Drop unused `HostOptions::has_extension` |

## Ownership rules

- Keep product protocol, Foreseer origins, tickets, and JS assets out of Jellium.
- Prefer additive generic APIs over Foreseer-named hooks.
- Runtime fixes must be stock-regressable without the host-extension feature when practical.

## Growth policy

When the delta grows (new commits or large file churn):

1. Run `scripts/patch-delta.sh docs/jellium-patch-delta.md`.
2. Update this manifest with the new SHAs and rationale.
3. Update `docs/integration-plan.md` ownership notes if the public API changed.
4. Re-run `scripts/boundary-audit.sh`.
