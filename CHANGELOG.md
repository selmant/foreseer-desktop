# Changelog

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
