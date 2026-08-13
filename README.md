# selector

A desktop selection box for Wayland. Drag on empty desktop and you get a rubber-band rectangle. It does absolutely nothing, but it is absolutely essential.

This is a compositor-agnostic take on [hyprselect](https://github.com/jmanc3/hyprselect), which is a Hyprland plugin which gets loaded into the compositor process, hooking Hyprland's internal render and input paths. That design cannot be ported to other compositors. selector is instead an ordinary Wayland client speaking only standard protocols, so it runs anywhere wlr-layer-shell exists.

## Building

```sh
nix develop # dev shell with cargo, clippy, rust-analyzer
cargo build
cargo test

nix build   # or build the package outright
```

Outside Nix all you need is a Rust toolchain (edition 2024, so 1.85+).

## Running

```sh
RUST_LOG=selector=debug cargo run
```

selector runs in the foreground until its surfaces are closed. Drag with the left button on empty desktop

## How it works

selector binds three globals, wl_compositor, wl_shm, and zwlr_layer_shell_v1, and creates one layer surface per output. Each surface is anchored to all four edges with an exclusive zone of `-1`, so it covers its entire output, panels included.

Rendering is software into a `wl_shm` buffer (`Argb8888`, premultiplied). There is no GPU dependency, and the surface is fully transparent whenever no drag is in progress.

## Known gaps

- A selection is confined to the output it started on; dragging across monitors needs output-layout awareness.
- Completed selections are logged only. `App::selection_completed` is the seam where they will be reported.
- Every frame repaints the full surface rather than tracking damage.
- `Config` is compiled-in defaults; there is no config file or CLI yet.

## License

MIT
