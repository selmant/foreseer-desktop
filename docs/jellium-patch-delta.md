# Jellium thin-fork patch delta

- upstream base: `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73`
- pin / HEAD: `ce5d4b5ce4952634fc7cabb18f7cf0a00b5e21b4`
- checkout: `/home/selmant/Projects/jellium-desktop-host-ext`

## Commits

ce5d4b5 fix: drop unused HostOptions::has_extension helper
0a12974 fix(wayland): log upstream mpv-proxy protocol errors
de3c381 fix(wayland): use full-buffer viewport during WSI resize
c9e8deb feat: add generic host-extension seam for embedding binaries

## Diffstat

```
 src/Cargo.lock                          |   3 +
 src/Cargo.toml                          |   1 +
 src/jfn_cef/Cargo.toml                  |   5 +
 src/jfn_cef/src/app.rs                  |   7 +
 src/jfn_cef/src/business_extension.rs   | 567 ++++++++++++++++++++++++++++++++
 src/jfn_cef/src/business_overlay.rs     |  21 ++
 src/jfn_cef/src/business_web.rs         |   5 +
 src/jfn_cef/src/client/events.rs        |   8 +
 src/jfn_cef/src/extension.rs            | 300 +++++++++++++++++
 src/jfn_cef/src/injection.rs            |  66 ++++
 src/jfn_cef/src/lib.rs                  |  10 +
 src/jfn_rust/Cargo.toml                 |   6 +
 src/jfn_rust/examples/host_extension.rs |  75 +++++
 src/jfn_rust/src/app.rs                 |  52 ++-
 src/jfn_rust/src/host.rs                |  74 +++++
 src/jfn_rust/src/lib.rs                 |   9 +
 src/wayland/src/layer.rs                |  14 +
 src/wayland/src/layer_actor.rs          |  11 +-
 src/wayland/src/mpv_proxy/app.rs        |  19 ++
 19 files changed, 1239 insertions(+), 14 deletions(-)
```

## File list

- src/Cargo.lock
- src/Cargo.toml
- src/jfn_cef/Cargo.toml
- src/jfn_cef/src/app.rs
- src/jfn_cef/src/business_extension.rs
- src/jfn_cef/src/business_overlay.rs
- src/jfn_cef/src/business_web.rs
- src/jfn_cef/src/client/events.rs
- src/jfn_cef/src/extension.rs
- src/jfn_cef/src/injection.rs
- src/jfn_cef/src/lib.rs
- src/jfn_rust/Cargo.toml
- src/jfn_rust/examples/host_extension.rs
- src/jfn_rust/src/app.rs
- src/jfn_rust/src/host.rs
- src/jfn_rust/src/lib.rs
- src/wayland/src/layer.rs
- src/wayland/src/layer_actor.rs
- src/wayland/src/mpv_proxy/app.rs
