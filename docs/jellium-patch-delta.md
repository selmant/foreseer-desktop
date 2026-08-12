# Jellium thin-fork patch delta

- upstream base: `28f2cf16a1f1b819884dd6a72919ca55bdf9bd73`
- pin / HEAD: `bce89c31b72d3c2d53bb0074e60d84a627a81cd4`
- checkout: `/home/selmant/Projects/jellium-desktop`

## Commits

bce89c3 fix(wayland): publish CEF copy/paste via native clipboard
478ce60 fix(cef): align dropdowns and GPU compositing
ecde360 fix: unmap hidden GPU CEF layers and serialize presentation
ce5d4b5 fix: drop unused HostOptions::has_extension helper
0a12974 fix(wayland): log upstream mpv-proxy protocol errors
de3c381 fix(wayland): use full-buffer viewport during WSI resize
c9e8deb feat: add generic host-extension seam for embedding binaries

## Diffstat

```
31 files changed, 1514 insertions(+), 78 deletions(-)
```

## File list

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
- src/platform_abi/src/lib.rs
- src/platform_abi/src/mpv_host.rs
- src/wayland/src/clipboard.rs
- src/wayland/src/layer.rs
- src/wayland/src/layer_actor.rs
- src/wayland/src/make_platform.rs
- src/wayland/src/mpv_host.rs
- src/wayland/src/mpv_proxy/app.rs
- src/wayland/src/mpv_proxy/mod.rs
- src/web/select-menu.js
