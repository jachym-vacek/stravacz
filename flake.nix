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
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "stravacz";
          inherit (workspace) version;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./crates
            ];
          };
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

        apps.default = flake-utils.lib.mkApp { drv = self.packages.${system}.default; };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
