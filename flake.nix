{
  description = "Terminal client for the Strava.cz canteen ordering system";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) lib;
        workspace = (lib.importTOML ./Cargo.toml).workspace.package;
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./crates
          ];
        };
        package = self.packages.${system}.default;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "stravacz";
          inherit (workspace) version;

          inherit src;
          cargoLock.lockFile = ./Cargo.lock;

          # aws-lc-rs (rustls' crypto provider) is built from C sources.
          nativeBuildInputs = [ pkgs.cmake ];
          # The TLS client loads system CA certificates even in tests.
          nativeCheckInputs = [ pkgs.cacert ];

          meta = {
            description = "Order school canteen meals from Strava.cz in your terminal";
            license = lib.licenses.mit;
            mainProgram = "stravacz-tui";
          };
        };

        apps.default = flake-utils.lib.mkApp { drv = package; };

        # `nix flake check`: build and test, clippy, and formatting.
        checks = {
          # The package's check phase runs `cargo test`.
          default = package;

          clippy = package.overrideAttrs (old: {
            pname = "stravacz-clippy";
            nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.clippy ];
            buildPhase = ''
              runHook preBuild
              cargo clippy --offline --frozen --all-targets --examples -- --deny warnings
              runHook postBuild
            '';
            doCheck = false;
            installPhase = "touch $out";
            dontFixup = true;
          });

          fmt =
            pkgs.runCommand "stravacz-fmt"
              {
                nativeBuildInputs = [
                  pkgs.cargo
                  pkgs.rustfmt
                  pkgs.nixfmt
                ];
              }
              ''
                cd ${src}
                cargo fmt --all --check
                nixfmt --check ${./flake.nix}
                touch $out
              '';
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ package ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };

        formatter = pkgs.nixfmt;
      }
    );
}
