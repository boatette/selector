# selector

A desktop selection box for Wayland. Drag on empty desktop and you get a rubber-band rectangle. It does absolutely nothing, but it is absolutely essential.

This is a compositor-agnostic take on [hyprselect](https://github.com/jmanc3/hyprselect), which is a Hyprland plugin which gets loaded into the compositor process, hooking Hyprland's internal render and input paths. That design cannot be ported to other compositors. selector is instead an ordinary Wayland client speaking only standard protocols, so it runs anywhere wlr-layer-shell exists.
