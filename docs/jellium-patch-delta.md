# Jellium thin-fork patch delta

- upstream base: `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73`
- pin / HEAD: `ffa6e228e1f66e615f2a03e48b18c05bb27d6140`
- checkout: `/home/selmant/Projects/jellium-desktop`

## Commits

ffa6e22 fix(windows): unmap hidden DComp visuals after playback
d04f440 fix(host-extension): preserve playback OSD interaction
bc3122d fix(host-extension): keep presentation terminology generic
0579346 fix(host-extension): prepare primary web before playback
db9ca5a fix(host-extension): inject host scripts without built-ins
ff71888 fix(host-extension): avoid shutdown callback deadlock
bf59292 docs: describe Foreseer runtime boundary
946e947 fix: gate host extension Arc import
bce89c3 fix(wayland): publish CEF copy/paste via native clipboard
478ce60 fix(cef): align dropdowns and GPU compositing
ecde360 fix: unmap hidden GPU CEF layers and serialize presentation
ce5d4b5 fix: drop unused HostOptions::has_extension helper
0a12974 fix(wayland): log upstream mpv-proxy protocol errors
de3c381 fix(wayland): use full-buffer viewport during WSI resize
c9e8deb feat: add generic host-extension seam for embedding binaries

## Diffstat

```
 README.md                                   |  35 +-
 src/Cargo.lock                              |   3 +
 src/Cargo.toml                              |   1 +
 src/jfn_cef/Cargo.toml                      |   5 +
 src/jfn_cef/src/app.rs                      |   9 +-
 src/jfn_cef/src/business_extension.rs       | 702 ++++++++++++++++++++++++++++
 src/jfn_cef/src/business_overlay.rs         |  21 +
 src/jfn_cef/src/business_web.rs             |   5 +
 src/jfn_cef/src/client.rs                   |   8 +
 src/jfn_cef/src/client/events.rs            |  45 ++
 src/jfn_cef/src/client_impl/context_menu.rs |  18 +
 src/jfn_cef/src/client_impl/keyboard.rs     |  17 +-
 src/jfn_cef/src/client_impl/render.rs       |  10 +
 src/jfn_cef/src/extension.rs                | 299 ++++++++++++
 src/jfn_cef/src/ffi.rs                      |   3 +
 src/jfn_cef/src/injection.rs                |  83 +++-
 src/jfn_cef/src/lib.rs                      |  10 +
 src/jfn_rust/Cargo.toml                     |   6 +
 src/jfn_rust/examples/host_extension.rs     |  75 +++
 src/jfn_rust/src/app.rs                     |  63 ++-
 src/jfn_rust/src/host.rs                    |  75 +++
 src/jfn_rust/src/lib.rs                     |   9 +
 src/jfn_rust/src/manager.rs                 |   6 +
 src/platform_abi/src/lib.rs                 |   6 +
 src/platform_abi/src/mpv_host.rs            |   4 +
 src/wayland/src/clipboard.rs                |  23 +-
 src/wayland/src/layer.rs                    |  14 +
 src/wayland/src/layer_actor.rs              |  52 +--
 src/wayland/src/make_platform.rs            |  19 +-
 src/wayland/src/mpv_host.rs                 |  20 +
 src/wayland/src/mpv_proxy/app.rs            |  19 +
 src/wayland/src/mpv_proxy/mod.rs            |   2 +-
 src/web/mpv-video-player.js                 |  19 +-
 src/web/select-menu.js                      |  81 ++--
 src/windows/src/render/layer.rs             |  19 +-
 src/windows/src/render/mod.rs               |  59 ++-
 36 files changed, 1737 insertions(+), 108 deletions(-)
```

## File list

- README.md
- src/Cargo.lock
- src/Cargo.toml
- src/jfn_cef/Cargo.toml
- src/jfn_cef/src/app.rs
- src/jfn_cef/src/business_extension.rs
- src/jfn_cef/src/business_overlay.rs
- src/jfn_cef/src/business_web.rs
- src/jfn_cef/src/client.rs
- src/jfn_cef/src/client/events.rs
- src/jfn_cef/src/client_impl/context_menu.rs
- src/jfn_cef/src/client_impl/keyboard.rs
- src/jfn_cef/src/client_impl/render.rs
- src/jfn_cef/src/extension.rs
- src/jfn_cef/src/ffi.rs
- src/jfn_cef/src/injection.rs
- src/jfn_cef/src/lib.rs
- src/jfn_rust/Cargo.toml
- src/jfn_rust/examples/host_extension.rs
- src/jfn_rust/src/app.rs
- src/jfn_rust/src/host.rs
- src/jfn_rust/src/lib.rs
- src/jfn_rust/src/manager.rs
- src/platform_abi/src/lib.rs
- src/platform_abi/src/mpv_host.rs
- src/wayland/src/clipboard.rs
- src/wayland/src/layer.rs
- src/wayland/src/layer_actor.rs
- src/wayland/src/make_platform.rs
- src/wayland/src/mpv_host.rs
- src/wayland/src/mpv_proxy/app.rs
- src/wayland/src/mpv_proxy/mod.rs
- src/web/mpv-video-player.js
- src/web/select-menu.js
- src/windows/src/render/layer.rs
- src/windows/src/render/mod.rs
