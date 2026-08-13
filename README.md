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

## Configuration

selector reads $XDG_CONFIG_HOME/selector/config.toml, falling back to ~/.config/selector/config.toml. Every key is optional and anything left out keeps its default.

A malformed or unreadable config is a startup error rather than a silent fallback

[`config.example.toml`](config.example.toml) documents every key at its default value:

```toml
# background, bottom, top, overlay
layer = "bottom"

# pixels the pointer must travel before a press counts as a drag
drag_threshold = 3.0

# outline thickness in pixels, 0 disables the outline
border_width = 1

# #rrggbb or #rrggbbaa, alpha defaults to ff when omitted
fill = "#4c9ed940"
border = "#4c9ed9cc"
```

`bottom` is the load-bearing default: it puts selector above the wallpaper but beneath every ordinary window, so a drag reaches it only when the desktop under the pointer is empty. Raising it to `top` or `overlay` makes selector swallow clicks meant for your windows.

## How it works

selector binds three globals, wl_compositor, wl_shm, and zwlr_layer_shell_v1, and creates one layer surface per output. Each surface is anchored to all four edges with an exclusive zone of `-1`, so it covers its entire output, panels included.

Rendering is software into a `wl_shm` buffer (`Argb8888`, premultiplied). There is no GPU dependency, and the surface is fully transparent whenever no drag is in progress.

## Known gaps

- A selection is confined to the output it started on; dragging across monitors needs output-layout awareness.
- Completed selections are logged only. `App::selection_completed` is the seam where they will be reported.
- Every frame repaints the full surface rather than tracking damage.
- The config is read once at startup; there is no reload and no CLI flags.

## License

MIT
