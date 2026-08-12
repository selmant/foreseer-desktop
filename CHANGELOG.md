# Changelog

## 0.2.2 — 2026-08-12

### Fixed

- Pin the Jellium runtime fix that lets default builds pass strict unused-import
  warnings while retaining the `host-extension` API.

## 0.2.1 — 2026-08-12

### Changed

- Integrates the generic Jellium `host-extension` runtime into the Foreseer
  Desktop product shell and pins its tested fork revision.
- Moves Foreseer protocol, authentication, session, controller, and injected
  web assets into the Desktop repository.
- Adds CI boundary and protocol gates for the pinned Jellium runtime.

## 0.2.0 — 2026-08-09

### Added

- Secure native bootstrap and setup flow using challenge-bound, short-lived Foreseer authentication tickets.

### Changed

- Pin the merged Jellium runtime revision used by release builds.

## 0.1.0 — 2026-08-02

First public source release of Foreseer Desktop.

### Supported

- Linux (Wayland primary; X11 best-effort), built from source against a pinned Jellium fork
- Discovery → native Jellyfin playback → return via Jellium `external-frontend`
- Foreseer ticket auth redemption into a private Jellyfin session

### Not yet

- Packaged AppImage / Flatpak / Windows / macOS installers
- Declared support for Windows or macOS (untested)

### Pins

- Jellium: see `jellium.rev` (`selmant/jellium-desktop`, `external-frontend` feature)
