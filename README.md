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

### Colours from a generator

The optional `colors` key points at a second file that supplies `fill` and `border`, so a theming engine can own the colours while everything else stays declarative:

```toml
# config.toml, managed by you
layer = "bottom"
border_width = 1
colors = "colors.toml"
```

```toml
# colors.toml, maanaged by generator
fill = "#89b4fa40"
border = "#89b4facc"
```

Relative paths resolve against the directory holding `config.toml`, and a leading `~` expands. Both keys in the colour file are optional, so a generator that only writes `fill` leaves `border` as configured. The colour file carries colours only anything else in it is an error.

A missing colour file is only a warning: the generator may not have run yet, and selector should not refuse to start over it. A malformed one is a hard error, like any other config.

selector reads its config once at startup, so point the generator's post-run hook at the service:

```sh
systemctl --user try-restart selector.service
```

### home-manager

The flake exposes a home-manager module as homeModules.selector:

```nix
{
  inputs.selector.url = "github:boatette/selector";
}
```

```nix
{ inputs, ... }:
{
  imports = [ inputs.selector.homeModules.selector ];

  programs.selector = {
    enable = true;

    settings = {
      layer = "bottom";
      border_width = 2;
      fill = "#4c9ed940";
      border = "#4c9ed9cc";
    };
  };
}
```

| Option         | Default               |                                                           |
| -------------- | --------------------- | --------------------------------------------------------- |
| enable         | false                 |                                                           |
| package        | this flake's selector |                                                           |
| systemd.enable | true                  | Run from a user service bound to graphical-session.target |
| settings       | {}                    | Written to $XDG_CONFIG_HOME/selector/config.toml          |

The systemd service is conditioned on WAYLAND_DISPLAY, so it stays inert outside a Wayland session. Set systemd.enable = false to install the binary and config but launch selector yourself. Note that the config is read once at startup, so changing settings needs a service restart.

Home-manager writes `config.toml` as a read-only symlink into the Nix store, which a theming tool cannot edit. `settings.colors` is the way round that — it names a runtime path, so the generator owns the colours and home-manager owns everything else:

```nix
programs.selector = {
  enable = true;

  settings = {
    layer = "bottom";
    border_width = 2;
    colors = "colors.toml";
  };
};
```

`colors.toml` is then read from `~/.config/selector/`, beside the symlink rather than inside the store. Leave `fill` and `border` unset here and the generator has sole say over them.

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
