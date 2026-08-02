# Foreseer Desktop

Native Foreseer shell backed by Jellium's opt-in external frontend runtime.
Foreseer Desktop owns product configuration and behavior; Jellium supplies the
native CEF/mpv playback mechanism.

**0.1.0 support:** Linux (Wayland primary, X11 best-effort), **from source**.
Windows/macOS and packaged installers are not released yet.

This binary links GPL-2.0-only Jellium code and is therefore GPL-2.0-only.
See [LICENSE](LICENSE).

## Requirements

- Adjacent [Jellium](https://github.com/selmant/jellium-desktop) checkout at the
  commit in [`jellium.rev`](jellium.rev) (default layout:
  `../jellium-desktop`)
- Rust stable, system `libmpv`, and the usual Linux native build deps
  (Wayland/X11, clang for bindgen)

```text
Projects/
  foreseer-desktop/   # this repo
  jellium-desktop/    # pinned fork, external-frontend branch tip in jellium.rev
```

```sh
git -C ../jellium-desktop fetch origin
git -C ../jellium-desktop checkout "$(tr -d '[:space:]' < jellium.rev)"
git -C ../jellium-desktop submodule update --init --recursive
```

## Run

```sh
cargo run
# optional frontend override:
FORESEER_URL=https://foreseer.example cargo run
```

Default frontend: `https://foreseer.selmantrabzon.com`.

## Test / lint

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

## Release pins

| Pin | Location |
| --- | --- |
| Version | `Cargo.toml` (`0.1.0`) |
| Jellium revision | `jellium.rev` |

CI checks out that Jellium revision as a sibling of this repo and runs format,
tests, and Clippy on Linux.

## Docs

Shared auth, playback routing, and lifecycle roadmap:
[docs/integration-plan.md](docs/integration-plan.md).
