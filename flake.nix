{
  description = "Terminal client for the Strava.cz canteen ordering system";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      advisory-db,
    }:
    let
      inherit (nixpkgs) lib;
      forAllSystems = lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          craneLib = crane.mkLib pkgs;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              (craneLib.fileset.commonCargoSources ./.)
              ./crates/stravacz-api/tests/fixtures
            ];
          };

          commonArgs = {
            pname = "stravacz";
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;
            inherit src;
            strictDeps = true;
            # The Secret Service keyring store links libdbus.
            nativeBuildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ];
            buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.dbus ];
          };

          # Dependencies only depend on Cargo.toml/Cargo.lock, so source edits
          # reuse this derivation in every build and check below.
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          package = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              # Tests run as separate checks.
              doCheck = false;
              meta = {
                description = "Order school canteen meals from Strava.cz in your terminal";
                license = lib.licenses.mit;
                mainProgram = "stravacz-tui";
              };
            }
          );
        in
        {
          packages.default = package;

          checks = {
            inherit package;

            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets --examples -- --deny warnings";
              }
            );

            tests = craneLib.cargoNextest (
              commonArgs
              // {
                inherit cargoArtifacts;
                # The TLS client loads system CA certificates even in tests.
                nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.cacert ];
              }
            );

            doctests = craneLib.cargoDocTest (commonArgs // { inherit cargoArtifacts; });

            doc = craneLib.cargoDoc (
              commonArgs
              // {
                inherit cargoArtifacts;
                RUSTDOCFLAGS = "--deny warnings";
              }
            );

            fmt = craneLib.cargoFmt { inherit src; };

            nixfmt = pkgs.runCommand "nixfmt-check" { nativeBuildInputs = [ pkgs.nixfmt ]; } ''
              nixfmt --check ${./flake.nix}
              touch $out
            '';

            audit = craneLib.cargoAudit { inherit src advisory-db; };
          };

          devShells.default = craneLib.devShell {
            checks = self.checks.${system};
            packages = [
              pkgs.rust-analyzer
              pkgs.cargo-nextest
            ];
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };

          formatter = pkgs.nixfmt-tree;
        };

      bySystem = forAllSystems perSystem;
    in
    {
      packages = lib.mapAttrs (_: s: s.packages) bySystem;
      checks = lib.mapAttrs (_: s: s.checks) bySystem;
      devShells = lib.mapAttrs (_: s: s.devShells) bySystem;
      formatter = lib.mapAttrs (_: s: s.formatter) bySystem;
    };
}
