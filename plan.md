# Rearchitect Foreseer Desktop Around a Thin Jellium Host-Extension Layer

## Summary

Rebuild the Jellium fork from current upstream `main`, keeping only generic native extension seams and independently upstreamable runtime fixes. Move all Foreseer-specific protocol, authentication, setup, private-session orchestration, playback correlation, JavaScript assets, and policy into `foreseer-desktop`; keep the hosted UI integration in `SeerrSuggestArr`.

Use a clean-break Foreseer-branded protocol v2. Develop on new branches/worktrees, preserve the existing v1 runtime and all uncommitted changes for rollback, and validate Linux Wayland/X11 before switching release pins.

Target result:

- Jellium contains no Foreseer names, protocol versions, ticket rules, event names, or product-specific scripts.
- Foreseer depends on a small, generic `host-extension` API instead of Jellium’s private CEF implementation.
- Updating Jellium becomes: fetch upstream, rebase a short generic patch stack, run stock and extension tests, update the pinned revision.
- Upstream PRs are submitted for generic seams/fixes but do not block Foreseer releases.

## Target Architecture and Interfaces

### Repository ownership

- `jellium-desktop`
  - Own CEF processes and browser layers, mpv, compositor ordering, atomic visibility/focus changes, window lifecycle, and playback lifecycle reporting.
  - Expose only opaque, thread-safe host-extension types from `jfn-rust`; never expose CEF pointers, mpv handles, arbitrary JavaScript execution, or platform handles.
  - Retain generic fixes only when they are valid for stock Jellium.

- `foreseer-desktop`
  - Own the native protocol, validation limits, capability declaration, setup flow, auth worker, state machine, request correlation, error mapping, injected Foreseer scripts, product configuration, diagnostics, and release pins.
  - Treat the hosted page and private Jellyfin renderer as untrusted inputs.
  - Keep secrets in native memory only and never include them in frontend events or logs.

- `SeerrSuggestArr`
  - Own `NativeRuntimeProvider`, web fallback, play-button behavior, auth-ticket issuance/redemption, and user-visible degraded/upgrade states.
  - Continue operating fully in an ordinary browser.

### Generic Jellium API

Replace `external-frontend`, `ExternalFrontend`, `HostAuthService`, `HostConfigService`, and `business_external` with an opt-in `host-extension` feature exposed by `jfn-rust`.

Public API:

```rust
HostOptions::with_extension(Arc<dyn HostExtension>)

trait HostExtension: Send + Sync {
    fn descriptor(&self) -> HostExtensionDescriptor;
    fn on_runtime_ready(&self, runtime: RuntimeHandle);
    fn admit_message(
        &self,
        source: ExtensionSource,
        origin: &str,
        payload: &[u8],
    ) -> bool;
    fn on_runtime_event(&self, event: RuntimeEvent);
}
```

Required opaque types:

- `HostExtensionDescriptor`
  - Initial frontend source: exact HTTP(S) URL or bounded trusted setup document.
  - Exact allowed frontend origin.
  - Trusted frontend and private-web injection assets supplied by the embedding binary.
  - Maximum inbound/outbound payload size, fixed at 16 KiB.
  - Whether the stock Jellium server overlay is enabled; Foreseer sets it to false.

- `ExtensionSource`
  - `Frontend`
  - `PrimaryWeb`

- `RuntimeEvent`
  - Frontend created/loaded/closed.
  - Primary web navigation/load.
  - Native playback `Started`, `Finished`, `Canceled`, or `Error`.
  - Runtime shutdown beginning.

- Cloneable `RuntimeHandle` operations:
  - Post a structured byte/JSON message to the frontend or primary web renderer through CEF process messaging, without string interpolation or arbitrary eval.
  - Navigate the primary web layer while atomically replacing its allowed origin.
  - Perform the one-way setup-document-to-hosted-URL transition.
  - Select `Presentation::Frontend` or `Presentation::PrimaryWeb`.
  - Request minimize, maximize toggle, fullscreen toggle, or coordinated shutdown.
  - No generic filesystem, token, mpv-command, shell, or raw browser APIs.

Jellium must implement presentation changes atomically:

- Frontend presentation: map/focus Foreseer, hide the private web layer, then deliver terminal events.
- Playback presentation: show/focus the private Jellyfin controller only after native playback reports `Started`; reassert mpv geometry.
- Never expose setup, login, error, or unexpected private-web routes.
- Preserve the default Jellium path exactly when no extension is configured.

The renderer transport exposes one internal send primitive. Trusted scripts supplied by Foreseer build the branded browser API on top of it. Jellium does not know Foreseer command or event names.

### Foreseer protocol v2

Make `foreseer-desktop/protocol/protocol-v2.json` canonical. Keep a byte-equivalent fixture in `SeerrSuggestArr`; Jellium carries no protocol fixture.

Expose:

```ts
interface ForeseerNativeV2 {
  readonly protocolVersion: 2;
  readonly hostName: 'foreseer-desktop';
  readonly hostVersion: string;
  readonly capabilities: readonly string[];
  send(command: NativeCommandV2): boolean;
}
```

Install it as the frozen, non-writable `window.foreseerNative`. Dispatch results through `foreseer:native-event`.

Use a discriminated command envelope:

```ts
type NativeCommandV2 =
  | { id: string; type: 'auth.challenge' }
  | { id: string; type: 'auth.complete'; ticket: string }
  | { id: string; type: 'session.clear' }
  | { id: string; type: 'play.item'; itemId: string }
  | { id: string; type: 'setup.check'; url: string; allowHttp: boolean }
  | { id: string; type: 'setup.save'; url: string; allowHttp: boolean }
  | { id: string; type: 'window.minimize' }
  | { id: string; type: 'window.toggle-maximize' }
  | { id: string; type: 'window.toggle-fullscreen' }
  | { id: string; type: 'app.quit' };
```

Events use `{ protocolVersion: 2, id, type, payload? }`. Preserve the current semantic lifecycle under v2: challenge, ready, accepted, resolving, starting, playing, stopped, finished, canceled, and safe error codes.

Validation requirements:

- Rust uses a `serde` tagged enum with unknown fields denied.
- Validate exact main-frame origin on every message.
- Retain current request-ID, item-ID, ticket, URL, token, and bootstrap-generation limits.
- Return `true` only after validation and successful enqueue into a bounded channel.
- Never block a CEF callback on network or state-machine work.
- Ignore v1 completely. If only `window.jelliumHost` exists, Seerr uses normal browser playback and may show a desktop-upgrade hint.

### Foreseer native controller

Split the current oversized binary logic into modules for:

- Product configuration and setup.
- Protocol v2 parsing and event serialization.
- Auth challenge/ticket worker.
- Private Jellyfin session controller.
- Pure application state machine.
- Jellium extension adapter and safe diagnostics.

Use explicit states:

`Starting → Setup | Authenticating → Ready → Resolving → Playing → Restoring → Ready`

with side states `Degraded` and `ShuttingDown`.

The controller, not Jellium, owns:

- Auth epochs and verifier rotation.
- Pending bootstrap generation.
- Active request ID and replacement/cancel behavior.
- Exact expected Jellyfin origin/server/user/generation.
- Whether a private-web message is valid for the current bootstrap.
- When to request frontend/player presentation.
- Safe mapping from internal failures to v2 error codes.

Move `external-host.js`, `jellyfin-session.js`, setup bridge logic, and external resume/play logic into Foreseer-owned injected assets. The private controller asset may use Jellyfin Web’s `ApiClient`, but communicates with Rust only through the structured extension transport.

## Implementation Phases

### 1. Preserve baselines and create clean development branches

- Do not alter the dirty Jellium checkout or its five modified files.
- Tag/archive the current v1 fork tip and Foreseer v0.2 baseline.
- Create a new Jellium worktree and branch from freshly fetched upstream `main`; the observed upstream tip is `28f2cf1`, but fetch again and record the actual SHA when implementation begins.
- Create a Foreseer v2 branch from `origin/main`, carrying forward the two remote commits currently missing locally without overwriting the modified `Cargo.lock`.
- Keep Seerr work on a dedicated branch from its current `develop`.
- Record old Jellium revision, new upstream base, new thin-fork revision, and protocol version in the migration document.

### 2. Build and prove the generic Jellium seam

- Add `host-extension` types and internal implementations without any Foreseer code.
- Pass trusted injection assets through CEF `extra_info` so renderer subprocesses receive them safely.
- Implement structured renderer/browser process messages in both directions.
- Add exact-origin and main-frame enforcement, payload limits, and dynamic primary-web origin replacement.
- Add opaque layer navigation/presentation operations and playback lifecycle callbacks.
- Add a minimal generic example extension that proves startup, messaging, presentation switching, and shutdown without Foreseer.
- Verify `HostOptions::default()` follows the stock startup, overlay, login, and playback path with no extension initialization.

### 3. Triage existing fork and working-tree changes

Do not cherry-pick the old external frontend feature wholesale. Classify every old commit or dirty hunk:

- Drop CEF severity mapping because upstream `28f2cf1` already contains it.
- Inherit upstream dependency, Chromium feature-switch, and SPA-navigation crash fixes.
- Port hidden GPU-layer unmapping, shared-texture/GPU-compositing behavior, zoom, Wayland viewport handling, and Wayland error diagnostics only when each is still missing and passes a stock-Jellium regression test.
- Submit each valid generic fix as a separate upstreamable commit/PR.
- Move the dirty `input-plugin.js` resume behavior into the Foreseer private-web controller asset.
- Leave obsolete `business_external`, Foreseer bridge/session assets, protocol fixture, auth traits, and Foreseer-specific lifecycle code behind.

### 4. Port Foreseer native behavior to the extension API

- Implement the v2 protocol parser and pure controller state machine first, using a mocked `RuntimeHandle`.
- Move current URL/config validation and setup services behind v2 commands.
- Adapt the existing bounded auth worker and one-time ticket redemption to controller events.
- Move session bootstrap installation and readiness acknowledgement into the Foreseer private-web asset/controller.
- Route playback through the private Jellyfin layer, keeping Jellyfin responsible for media resolution, resume position, stream selection, and reporting.
- Subscribe to Jellium’s native playback events for authoritative surface switching and terminal restoration.
- Remove Foreseer’s dependency on all old external-frontend types and functions.
- Depend only on `jfn-rust` with `host-extension`; keep the adjacent path dependency for development and `jellium.rev` as the exact release/CI pin.

### 5. Convert Seerr to protocol v2

- Replace `window.jelliumHost` declarations with `window.foreseerNative`.
- Update `NativeRuntimeProvider` to send validated v2 envelopes and listen to `foreseer:native-event`.
- Keep SSR at `web`; detect v2 only after hydration.
- Treat missing, malformed, v1, wrong-host, or capability-incomplete bridges as browser fallback.
- Preserve at-most-one queued play while authenticating, request replacement, stale-event rejection, logout/account-switch session clearing, and degraded retry behavior.
- Change desktop ticket issue/redeem validation from protocol version 1 to 2. No database migration is required because `protocolVersion` already exists; outstanding v1 tickets expire naturally within 60 seconds.
- Keep existing HTTP endpoints and bootstrap response fields unless a failing test demonstrates a required wire change.

### 6. Establish fork-maintenance gates

Add a boundary check owned by Foreseer CI that compares the pinned thin fork against its recorded upstream base and fails when:

- Jellium production code or assets contain `Foreseer`, `foreseerNative`, protocol version constants, ticket formats, endpoint paths, or Foreseer event names.
- The fork adds protocol fixtures or product-specific JavaScript.
- A commit is neither part of the approved generic host-extension patch stack nor a separately documented generic runtime fix.
- Foreseer imports anything from Jellium other than the public `jfn-rust` extension API.

Generate a patch-delta report on every pin update showing changed commits, files, and line counts. Any growth in the extension patch requires an explicit update to the boundary manifest and architecture document.

### 7. Documentation and upstream work

- Rewrite `docs/integration-plan.md` around protocol v2 and the new ownership boundary.
- Add an upgrade runbook: fetch upstream, create/rebase thin branch, run patch audit, run stock Jellium tests, run Foreseer tests, perform Linux smoke matrix, update `jellium.rev`.
- Document the opaque Jellium API next to its public Rust types.
- Submit non-blocking upstream PRs in small units: generic runtime fixes first, structured host-extension transport second, presentation/lifecycle extension support third.
- Do not include Foreseer screenshots, endpoints, names, or protocol details in upstream PRs.

## Test and Acceptance Plan

### Automated Jellium tests

Run stock/default and `host-extension` configurations:

- `just fmt-check`
- `just lint`
- `just test`
- Feature-specific Rust tests and Clippy for both default and all-feature builds.

Cover:

- Default startup never constructs extension layers.
- Exact scheme/host/port origin checks, including evil subdomains.
- Main-frame-only IPC.
- Oversized, malformed, unknown, and non-UTF-8 payload rejection.
- Renderer subprocess receives only configured assets/profile.
- No callback blocks the CEF UI thread.
- Setup authority is removed after the one-way navigation.
- Presentation changes update visibility, focus, stacking, and mpv geometry together.
- Playback and shutdown events remain ordered and safe after layer closure.
- Stock Jellium setup, login, playback, Back, and quit remain functional.

### Automated Foreseer Desktop tests

Run formatting, tests, Clippy, and the protocol harness. Add controller tests for:

- Setup success/failure and stale async setup callbacks.
- Auth challenge, success, timeout, wrong verifier, expired/used ticket, account switch, and logout.
- Bootstrap server/user/generation match and mismatch.
- Secrets absent from errors, logs, protocol events, and diagnostics.
- Play before ready, queued play, accepted play, replacement, double-click, stale terminal event, early Jellyfin failure, native completion, Back, and renderer reload.
- Presentation restoration occurs before terminal frontend delivery.
- Shutdown cancels workers without deadlock.
- v2 fixture and package version consistency.
- Jellium public API can be updated without importing internal crates or CEF/platform types.

### Automated Seerr tests

Run:

- `pnpm format:check`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`
- `pnpm build`
- Migration checks.

Cover:

- Ordinary browser behavior.
- v1-only host falls back to browser playback.
- Valid v2 discovery and capability gating.
- Malformed host/event rejection.
- Ticket issue/redeem v2 success and all current security failures.
- Queued play, replacement, stale events, timeout, degraded retry, logout, and account switch.
- Unsupported providers and trailers retain browser behavior.
- No token/device ID enters frontend responses or events.

### Linux release acceptance

Test Wayland and X11 against the configured production Foreseer and Jellyfin versions:

- Existing Foreseer cookie reuse and each supported login path.
- Discovery/library play, resume, episode selection, and replacement play.
- Visible video, audio, Jellyfin OSD controls, focus, keyboard/mouse input.
- Back/stop restores the exact Foreseer route and scroll/navigation state.
- No Jellyfin setup/login/error flash before, during, or after playback.
- Resize, maximize, fullscreen, mixed DPI, zoom, suspend/resume, and SPA navigation.
- Shared-texture path, fallback paint path, and explicit GPU-compositing opt-out.
- Network/auth failure recovery and renderer reload.
- Window close, menu quit, sidebar quit, and repeated startup.
- Fifty discovery → play → Back cycles with no hidden audio, leaked surfaces, lost focus, deadlock, or increasing process/resource counts.
- Repeat stock Jellium login/play/return smoke tests on Wayland and X11.

## Rollout and Completion Criteria

- Deploy the Seerr v2-capable web application first. Old v1 desktops safely receive browser fallback rather than native playback.
- Release the new Foreseer Desktop build only after the full Linux gate passes.
- Keep the archived v1 branches/tags and previous binary available for rollback.
- Switch the fork’s default branch and update `jellium.rev` only after the new worktree passes both stock Jellium and Foreseer acceptance.
- Monitor safe state-transition/error-code counts and crash rates; never record request payloads, tickets, verifiers, tokens, device IDs, or bootstrap envelopes.
- Migration is complete only when the old Jellium `external-frontend` implementation is absent from the release branch, the fork boundary audit passes, protocol v2 works end-to-end, stock Jellium remains green, and the 50-cycle Linux soak passes.

## Assumptions and Defaults

- Optimize for a thin generic fork, not a zero-fork design.
- Linux Wayland and X11 are the first release gate; Windows/macOS must continue compiling where CI supports them but are not v2 end-to-end blockers.
- Protocol v2 is a clean break with Foreseer branding; no dual-protocol native implementation.
- Upstream PR submission is required, but upstream acceptance is not a release blocker.
- Current dirty files and v1 history are user-owned and must be preserved in their existing worktrees.
- The existing auth-ticket database model and endpoint paths remain valid; only their protocol-version validation changes.
