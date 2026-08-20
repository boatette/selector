# selector

A desktop selection box for Wayland. Drag on empty desktop and you get a rubber-band rectangle. It does absolutely nothing, but it is absolutely essential.

This is a compositor-agnostic take on [hyprselect](https://github.com/jmanc3/hyprselect), a Hyprland plugin that loads into the compositor process and hooks its internal render and input paths. That design cannot be ported elsewhere. selector is an ordinary Wayland client speaking only standard protocols, so it runs anywhere wlr-layer-shell exists.

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

selector runs in the foreground until its surfaces are closed. Drag with the left button on empty desktop.

## Configuration

selector reads `$XDG_CONFIG_HOME/selector/config.toml`, falling back to `~/.config/selector/config.toml`. Every key is optional and anything left out keeps its default. A malformed or unreadable config is a startup error rather than a silent fallback, and the config is read once at startup, so changes need a restart.

| Key              | Default       | Meaning                                                        |
| ---------------- | ------------- | -------------------------------------------------------------- |
| `layer`          | `"bottom"`    | `background`, `bottom`, `top` or `overlay`                     |
| `drag_threshold` | `3.0`         | pixels the pointer must travel before a press counts as a drag |
| `border_width`   | `1`           | outline thickness in pixels, `0` disables the outline          |
| `corner_radius`  | `0`           | corner rounding in pixels, clamped to half the shorter side    |
| `blur`           | `false`       | ask the compositor to blur behind the selection                |
| `fill`           | `"#4c9ed940"` | interior colour, `#rrggbb` or `#rrggbbaa`                      |
| `border`         | `"#4c9ed9cc"` | outline colour, `#rrggbb` or `#rrggbbaa`                       |
| `colors`         | unset         | path to a second file supplying `fill` and `border`            |

Also see [`config.example.toml`](config.example.toml)

`bottom` is the load-bearing default: it puts selector above the wallpaper but beneath every ordinary window, so a drag reaches it only when the desktop under the pointer is empty. Raising it to `top` or `overlay` makes selector swallow clicks meant for your windows.

### Blur

A Wayland client cannot see what is behind it, so blur is something selector asks for rather than draws. It names the selection as a region to blur through [ext-background-effect-v1](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/tree/main/staging/ext-background-effect), and the compositor blurs what it paints underneath. The blur shows through wherever `fill` is translucent, so the alpha in `fill` is what tunes it and an opaque `fill` hides it entirely. On a compositor without the protocol, `blur = true` logs a warning at startup and changes nothing else.

niri implements it, but also wants a rule allowing the effect for our layer surface:

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

The `colors` key points at a second file supplying `fill` and `border`, so a theming engine can own the colours while everything else stays declarative:

```toml
# config.toml, managed by you
layer = "bottom"
border_width = 1
colors = "colors.toml"
```

```toml
# colors.toml, managed by the generator
fill = "#89b4fa40"
border = "#89b4facc"
```

Relative paths resolve against the directory holding `config.toml`, and a leading `~` expands. Both keys are optional, so a generator that only writes `fill` leaves `border` as configured, and anything other than those two keys in the file is an error. A missing colour file is only a warning, since the generator may not have run yet; a malformed one is a hard error.

Point the generator's post-run hook at the service so the new colours are picked up:

```sh
systemctl --user try-restart selector.service
```

### home-manager

The flake exposes a home-manager module as `homeModules.selector`:

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

| Option           | Default                 | Meaning                                                     |
| ---------------- | ----------------------- | ----------------------------------------------------------- |
| `enable`         | `false`                 | install the package and, unless disabled, the user service  |
| `package`        | this flake's `selector` | the package to use                                          |
| `systemd.enable` | `true`                  | run from a user service bound to `graphical-session.target` |
| `settings`       | `{}`                    | written to `$XDG_CONFIG_HOME/selector/config.toml`          |

The service is conditioned on `WAYLAND_DISPLAY`, so it stays inert outside a Wayland session. Set `systemd.enable = false` to install the binary and config but launch selector yourself.

home-manager writes `config.toml` as a read-only symlink into the Nix store, which a theming tool cannot edit. `settings.colors` is the way round that: it names a runtime path, so the generator owns the colours and home-manager owns everything else.

```nix
programs.selector.settings = {
  layer = "bottom";
  border_width = 2;
  colors = "colors.toml";
};
```

`colors.toml` is then read from `~/.config/selector/`, beside the symlink rather than inside the store. Leave `fill` and `border` unset here and the generator has sole say over them.

## License

MIT
