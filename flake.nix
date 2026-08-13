{
  description = "a desktop selection box for Wayland compositors";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        selector = pkgs.rustPlatform.buildRustPackage {
          pname = "selector";
          version = (fromTOML (builtins.readFile ./Cargo.toml)).package.version;

          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "A desktop selection box for Wayland";
            homepage = "https://github.com/boatette/selector";
            license = pkgs.lib.licenses.mit;
            mainProgram = "selector";
            platforms = pkgs.lib.platforms.linux;
          };
        };

        default = selector;
      });

      homeModules = rec {
        selector = import ./nix/home-manager.nix self;
        default = selector;
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];

          env = {
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            RUST_BACKTRACE = "1";
          };
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
