self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.selector;
  tomlFormat = pkgs.formats.toml { };

  settings = lib.filterAttrs (_: value: value != null) cfg.settings;

  colorOption =
    description:
    lib.mkOption {
      type = lib.types.nullOr (lib.types.strMatching "#?[0-9a-fA-F]{6}([0-9a-fA-F]{2})?");
      default = null;
      example = "#4c9ed940";
      inherit description;
    };
in
{
  options.programs.selector = {
    enable = lib.mkEnableOption "selector, a desktop selection box for Wayland";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.selector;
      defaultText = lib.literalExpression "selector.packages.\${system}.selector";
      description = "The selector package to use";
    };

    systemd.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to run selector from a systemd user service bound to graphical-session.target";
    };

    settings = lib.mkOption {
      default = { };
      example = lib.literalExpression ''
        {
          layer = "bottom";
          border_width = 2;
          fill = "#4c9ed940";
          border = "#4c9ed9cc";
        }
      '';
      description = "Settings written to $XDG_CONFIG_HOME/selector/config.toml";
      type = lib.types.submodule {
        freeformType = tomlFormat.type;

        options = {
          layer = lib.mkOption {
            type = lib.types.nullOr (
              lib.types.enum [
                "background"
                "bottom"
                "top"
                "overlay"
              ]
            );
            default = null;
            example = "bottom";
            description = "which layer the surfaces sit on: background, bottom, top, overlay";
          };

          drag_threshold = lib.mkOption {
            type = lib.types.nullOr (lib.types.either lib.types.ints.unsigned lib.types.float);
            default = null;
            example = 3.0;
            description = "how far the pointer must travel, in pixels, before a press counts as a drag";
          };

          border_width = lib.mkOption {
            type = lib.types.nullOr lib.types.ints.unsigned;
            default = null;
            example = 1;
            description = "outline thickness in pixels, 0 disables the outline";
          };

          corner_radius = lib.mkOption {
            type = lib.types.nullOr lib.types.ints.unsigned;
            default = null;
            example = 8;
            description = "corner rounding in pixels, 0 is square corners, clamped to half the shorter side";
          };

          blur = lib.mkOption {
            type = lib.types.nullOr lib.types.bool;
            default = null;
            example = true;
            description = "ask the compositor to blur what is behind the selection, needs ext-background-effect and a translucent fill";
          };

          fill = colorOption "interior colour of the selection box, as #rrggbb or #rrggbbaa";
          border = colorOption "outline colour of the selection box, as #rrggbb or #rrggbbaa";

          colors = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            example = "~/.config/selector/colors.toml";
            description = "path to a second file supplying fill and border";
          };
        };
      };
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."selector/config.toml" = lib.mkIf (settings != { }) {
      source = tomlFormat.generate "selector-config.toml" settings;
    };

    systemd.user.services.selector = lib.mkIf cfg.systemd.enable {
      Unit = {
        Description = "A desktop selection box for Wayland compositors";
        Documentation = "https://github.com/boatette/selector";
        PartOf = [ "graphical-session.target" ];
        After = [ "graphical-session.target" ];
        ConditionEnvironment = "WAYLAND_DISPLAY";
      };

      Service = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
        RestartSec = 1;
      };

      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
