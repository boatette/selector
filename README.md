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

# corner rounding in pixels, 0 is square corners
corner_radius = 0

# ask the compositor to blur what is behind the selection
blur = false

# #rrggbb or #rrggbbaa, alpha defaults to ff when omitted
fill = "#4c9ed940"
border = "#4c9ed9cc"
```

`bottom` is the load-bearing default: it puts selector above the wallpaper but beneath every ordinary window, so a drag reaches it only when the desktop under the pointer is empty. Raising it to `top` or `overlay` makes selector swallow clicks meant for your windows.

### Rounded corners

`corner_radius` rounds the selection box, antialiased, and is clamped to half the shorter side, so a radius larger than the selection degrades to a pill and then to a circle rather than glitching. The radius names the *outer* edge; the fill's curve is drawn concentric with it at `corner_radius - border_width`, so the outline keeps the same thickness on the curve as on the straight edges. A `border_width` wider than the radius floors the inner curve at zero, giving a square-cornered fill inside a rounded ring.

With `blur = true` the region handed to the compositor follows the rounded outline too, decomposed into scanlines, so the blur does not square off the corners. A `wl_region` is rectangle algebra with no antialiasing, so its edge is a hard staircase against the painted one, half a pixel apart at worst.

A selection that runs past a screen edge keeps its true geometry: each output draws the part of the box that falls on it and clips the rest, so the corners stay where the drag put them instead of being re-rounded against the edge. On the last screen in the layout that means a straight cut at the edge, and on a monitor in the middle of a cross-screen drag it means no corner at all.

### Blur

A Wayland client cannot see what is behind it, so blur is something selector asks for rather than draws. With `blur = true` selector uses [ext-background-effect-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/tree/main/staging/ext-background-effect) to name the selection rectangle as a region to blur, and the compositor blurs what it paints underneath. The blur shows through wherever `fill` is translucent, so an opaque `fill` hides it completely and the alpha in `fill` is what tunes it. Everything else about the effect (radius, passes, whether it happens at all) is compositor policy, the protocol has no knobs.

The region follows the drag and is withdrawn when the selection ends, so nothing is blurred while the desktop is idle.

On a compositor without the protocol, `blur = true` logs a warning at startup and selector draws as it always did. niri implements it, but also wants a rule allowing the effect for our layer surface:

```kdl
layer-rule {
    match namespace="^selector$"

    background-effect {
        blur true
    }
}
```

Hyprland has no such protocol and blurs layer surfaces by rule instead, keyed off the same namespace, with no help needed from `blur`:

```ini
layerrule = blur, selector
# without this the whole transparent surface is blurred, not just the rectangle
layerrule = ignorealpha 0.1, selector
```

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

selector binds three globals, wl_compositor, wl_shm, and zwlr_layer_shell_v1, plus ext_background_effect_manager_v1 when it is there and blur is on, and creates one layer surface per output. Each surface is anchored to all four edges with an exclusive zone of `-1`, so it covers its entire output, panels included.

Rendering is software into a `wl_shm` buffer (`Argb8888`, premultiplied). There is no GPU dependency, and the surface is fully transparent whenever no drag is in progress.

There is one selection for the whole layout, held in global compositor coordinates. Pointer events are surface-local, so each event is lifted into that space using the output's position from xdg-output (falling back to wl_output's geometry), and every surface subtracts its own origin again to draw. A drag therefore survives crossing a monitor boundary: the button holds an implicit grab on the surface it started on, whose motion coordinates simply run off the edge, and each output repaints whenever its own view of the selection changes. Should a compositor break the grab and hand focus to the next output instead, the drag continues there rather than being cancelled.

## Known gaps

- Completed selections are logged only. `App::selection_completed` is the seam where they will be reported.
- Every frame repaints the full surface rather than tracking damage.
- The config is read once at startup; there is no reload and no CLI flags.

## TODO:

- Test on more wayland compositors
- Fix issues in [Known gaps](#known-gaps)

## License

MIT
