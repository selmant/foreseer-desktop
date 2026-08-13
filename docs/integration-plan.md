# Foreseer Desktop Integration Plan (protocol v1)

Status: host-extension integration is released as Foreseer Desktop `v0.2.7`.
Automated boundary and protocol gates are enabled; Linux Wayland/X11 acceptance
and the 50-cycle playback soak remain manual release gates.

## Goal

One Foreseer product in two environments:

- **Browser:** full web app; play actions use ordinary Jellyfin links.
- **Foreseer Desktop:** same hosted UI detects `window.foreseerNative` (protocol
  v1), reuses the signed-in user's linked Jellyfin identity, and plays through
  Jellium/mpv in one window. Terminal playback restores the Foreseer route
  without exposing private Jellyfin Web.

Foreseer Desktop + the maintained thin Jellium fork are the supported native
stack. Product stability comes first; upstreaming is opportunistic.

## Non-goals

- Do not move the Foreseer product into Jellium.
- Do not reimplement Jellyfin Web playback negotiation in Foreseer.
- Do not expose mpv, filesystem, shell, CEF handles, or Jellyfin tokens to the
  hosted page.
- Do not make the browser build depend on the desktop binary.
- Do not put Foreseer protocol, origins, tickets, or product JS into Jellium.

## Ownership

| Concern | Owner |
| --- | --- |
| Discovery, requests, library UI, ordinary browser play | SeerrSuggestArr |
| `window.foreseerNative` detection + ticket issue (`protocolVersion: 1`) | SeerrSuggestArr |
| Protocol v1, controller, injected assets, config, pins | foreseer-desktop |
| CEF/mpv/compositor/window + generic `HostExtension` seam | Jellium thin fork |
| Jellyfin playback negotiation / resume | Jellyfin Web via Jellium private layer |

## Runtime shape

```mermaid
flowchart LR
    F[Hosted Foreseer UI] -->|HTTPS cookie session| S[Foreseer server]
    F -->|foreseerNative.send| D[Foreseer Desktop extension]
    D -->|HostExtension / RuntimeHandle| J[Jellium thin fork]
    J -->|private authenticated layer| W[Jellyfin Web]
    W --> M[mpv]
    S -->|single-use redeem| D
    F -. browser .-> L[Jellyfin link]
```

## Protocol v1 (product boundary)

Canonical fixture: [`protocol/protocol-v1.json`](../protocol/protocol-v1.json),
with a byte-equivalent copy in the Foreseerr repository.

- Global: frozen `window.foreseerNative` with `protocolVersion: 1`,
  `hostName: 'foreseer-desktop'`, and `send(command)`.
- Events: `foreseer:native-event` with `{ protocolVersion, id, type, ... }`.
- Commands are intent-level only (`auth.*`, `play.item`, `session.clear`,
  window/app/setup). No tokens, device IDs, or mpv commands on the page wire.
- Absent / unusable `foreseerNative` → ordinary browser playback.
- Resume ticks stay Jellyfin-owned (`startPositionTicksInProtocol: false`).

## Opaque Jellium API (public surface)

Foreseer may import only the `host-extension` exports from `jfn-rust`:

- `HostOptions::with_extension`
- `HostExtension` / `HostExtensionDescriptor`
- `ExtensionSource` / `FrontendSource`
- `Presentation` (`Frontend` / `PrimaryWebPreparing` / `PrimaryWeb`) /
  `RuntimeEvent` / `RuntimeHandle`
- `jfn_app_main_with`
- related config errors / payload limit constants

Everything else (protocol parsing, auth redeem, setup HTML, product JS) lives in
Foreseer. Stock Jellium with no extension configured must behave as upstream.

## Fork maintenance

- Pin: [`jellium.rev`](../jellium.rev)
- Upstream base: [`jellium.upstream-base`](../jellium.upstream-base)
- Approved commits: [`docs/jellium-patch-manifest.md`](jellium-patch-manifest.md)
- Upgrade steps: [`docs/upgrade-runbook.md`](upgrade-runbook.md)
- Gates: `scripts/boundary-audit.sh`, `scripts/patch-delta.sh`

## Security notes

- Hosted Foreseer is untrusted input even when expected.
- Exact-origin allowlisting and payload size limits stay in the native host.
- Tickets are single-use, short-lived, challenge-bound; never log tokens/tickets.
- Frontend responses must not include access tokens or device IDs.

## Packaging / soak

Linux from-source is the current support surface. Packaged installers and the
Phase 7 Wayland/X11 50-cycle soak remain acceptance gates before calling the
migration complete.
