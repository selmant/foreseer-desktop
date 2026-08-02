# Foreseer Desktop

Native Foreseer shell backed by Jellium's opt-in external frontend runtime.
Foreseer Desktop owns product configuration and behavior; Jellium supplies the
native CEF/mpv playback mechanism.

For local development, `jfn-rust` is referenced from the adjacent
`jellium-desktop` checkout. Set `FORESEER_URL` to override the hosted frontend:

```sh
FORESEER_URL=https://foreseer.example cargo run
```

This binary links GPL-2.0-only Jellium code and is therefore GPL-2.0-only.

The proposed shared-auth, universal web/native playback, and application
lifecycle roadmap is documented in [docs/integration-plan.md](docs/integration-plan.md).
