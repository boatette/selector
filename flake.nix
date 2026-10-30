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

          # only what cargo reads, so edits to the docs or the nix files don't rebuild it
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              ./benches
              # the config tests include_str! it
              ./config.example.toml
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "A desktop selection box for Wayland";
            homepage = "https://github.com/boatette/selector-flake";
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
