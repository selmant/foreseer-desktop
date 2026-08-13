# Foreseer Desktop

Native Foreseer shell backed by Jellium's opt-in `host-extension` runtime.
Foreseer Desktop owns protocol v1, product policy, and injected assets; Jellium
supplies CEF/mpv and a generic extension seam.

**0.2 support:** Linux (Wayland primary, X11 best-effort), **from source**.
Windows/macOS and packaged installers are not released yet.

This binary links GPL-2.0-only Jellium code and is therefore GPL-2.0-only.
See [LICENSE](LICENSE).

## How this fits the Foreseer product

Foreseer Desktop is an optional client for the hosted
[Foreseerr](https://github.com/selmant/foreseerr) application. It does not
replace or bundle the web app, run a separate request server, or own the user's
media-account configuration.

| Component | Owns |
| --- | --- |
| [Foreseerr](https://github.com/selmant/foreseerr) | Hosted UI, sign-in, linked Jellyfin identity, discovery, requests, library browsing, and browser fallback. |
| Foreseer Desktop | Native protocol v1, secure desktop bootstrap, desktop configuration, and the product release pin. |
| [Jellium](https://github.com/selmant/jellium-desktop) | Generic CEF/mpv runtime, compositor/window lifecycle, and the `host-extension` API. |
| Jellyfin Web | Media resolution, resume position, stream selection, and playback reporting. |

The same Foreseerr page works in both environments. In a browser, play controls
remain ordinary links. In this Desktop client, a compatible signed-in Jellyfin
play action is passed to the native runtime; unsupported media and any native
failure retain the browser fallback. User-facing setup and troubleshooting are
documented in Foreseerr's [Native Desktop guide](https://github.com/selmant/foreseerr/blob/develop/docs/using-seerr/native-desktop.md).

## Requirements

- Adjacent [Jellium](https://github.com/selmant/jellium-desktop) checkout at the
  commit in [`jellium.rev`](jellium.rev) (default layout:
  `../jellium-desktop`)
- Rust stable, system `libmpv`, and the usual Linux native build deps
  (Wayland/X11, clang for bindgen)

```text
Projects/
  foreseer-desktop/              # this repo
  jellium-desktop/               # pinned thin fork tip in jellium.rev
```

```sh
git -C ../jellium-desktop fetch origin
git -C ../jellium-desktop checkout "$(tr -d '[:space:]' < jellium.rev)"
git -C ../jellium-desktop submodule update --init --recursive
JELLIUM_DIR=../jellium-desktop ./scripts/boundary-audit.sh
```

Architecture: [docs/integration-plan.md](docs/integration-plan.md).  
Fork upgrades: [docs/upgrade-runbook.md](docs/upgrade-runbook.md).

## Configuration & CLI

Foreseer Desktop persists its configuration in a standard OS config directory:
- **Linux**: `~/.config/Foreseer/config.json`
- **macOS**: `~/Library/Application Support/com.selmantrabzon.Foreseer/config.json`
- **Windows**: `%APPDATA%\selmantrabzon\Foreseer\config.json`

```json
{
  "server_url": "https://foreseer.example.com",
  "allow_insecure_http": false
}
```

### CLI Commands & Environment Variables

```sh
# Run with default or saved server URL:
cargo run

# Launch the graphical server setup GUI:
cargo run -- --setup

# View current configuration and file location:
cargo run -- --show-config

# Set a new default server URL:
cargo run -- --set-url https://foreseer.example.com

# Allow HTTP (non-HTTPS) server URL:
cargo run -- --set-url http://192.168.1.50:5055 --allow-http

# Temporary environment variable override (does not modify config.json):
FORESEER_URL=https://foreseer.example cargo run
```

## Test / lint

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Deterministic protocol/integration harness (no network, CEF, mpv, or secrets):
node scripts/protocol-v1-harness.mjs
```

Protocol v1 is canonical in `protocol/protocol-v1.json` (byte-equivalent copy in
Foreseerr). The Desktop client accepts only protocol v1; a browser or an
incompatible native runtime falls back to ordinary web playback.

The harness covers fixture shape, command set, and package version. Before a
release, run the Wayland and X11 visible-video/audio/focus matrix (including
resize, fullscreen, mixed DPI, suspend/resume, and renderer recovery), then a
50-cycle discovery → play → Back soak while checking for hidden audio, surface
leaks, focus loss, and Jellyfin UI flashes.

## Release pins

| Pin | Location |
| --- | --- |
| Version | `Cargo.toml` (`0.2.7`) |
| Jellium revision | `jellium.rev` |

CI checks out that Jellium revision as a sibling of this repo and runs format,
tests, and Clippy on Linux.

## Docs

Shared auth, playback routing, and lifecycle roadmap:
[docs/integration-plan.md](docs/integration-plan.md).
