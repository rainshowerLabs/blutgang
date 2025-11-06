{
  description = "Blutgang - The WD40 of Ethereum load balancers";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = { self, ... }@inputs:
    inputs.flake-utils.lib.eachDefaultSystem
      (system:
        let
          inherit (pkgs) fenix;
          inherit (pkgs.lib) optionalAttrs optional sources;
          inherit (pkgs.lib.fileset) unions fileFilter toSource;

          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.fenix.overlays.default ];
          };

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain fenix.stable.toolchain;
          craneLibNightly = craneLib.overrideToolchain (fenix.fromToolchainFile {
            file = ./.rust-toolchain.toml;
            sha256 = "sha256-5fekoWL33RFwtNPbtEHYUB4UrqSjmGWQ4ju6enZexVM=";
          });
          craneLibLlvm = craneLib.overrideToolchain
            (fenix.complete.withComponents [
              "cargo"
              "llvm-tools"
              "rustc"
            ]);

          src =
            let
              root = ./.;
              fileset = unions [
                (craneLib.fileset.commonCargoSources root)
                (fileFilter (file: file.hasExt "md") root)
                ./LICENSE
              ];
            in
            toSource {
              inherit root fileset;
            };

          buildArgs = {
            inherit src;
            strictDeps = true;

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook
              llvmPackages.bintools
              libclang.lib
            ];
            buildInputs = with pkgs; [
              openssl.dev
              rocksdb
            ];

            # Used by build.rs in the rocksdb-sys crate. If we don't set these, it would
            # try to build RocksDB from source.
            ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";
            # https://github.com/nix-community/fenix/issues/206
            # Tells rust which linker to use, in this case bintools, containing lld.
            # The problem doesn't seem to exist on MacOS.
            env = optionalAttrs (system == "x86_64-linux") {
              RUSTFLAGS = "-C link-self-contained=-linker";
            };
          };

          # Build *just* the cargo dependencies, so we can reuse
          # all of that work (e.g. via cachix) when running in CI
          cargoArtifacts = craneLib.buildDepsOnly buildArgs;

          mkBlutgang = { features ? [ ] }: craneLib.buildPackage (buildArgs // {
            inherit cargoArtifacts;

            # Only patchelf for the actual package
            nativeBuildInputs = with pkgs; buildArgs.nativeBuildInputs
            ++ optional stdenv.isLinux [
              autoPatchelfHook
            ];
            doCheck = false;

            CARGO_PROFILE = "maxperf";
          } // optionalAttrs (builtins.length features > 0) {
            cargoExtraArgs = "--locked --features ${builtins.concatStringsSep " " features}";
          });

          blutgang = mkBlutgang { };
          blutgang-systemd = mkBlutgang { features = [ "journald" ]; };

          mkGuestPlatform = system: builtins.replaceStrings [ "darwin" ] [ "linux" ] system;
          dockerImage =
            let
              inherit (pkgs) blutgang;
              pkgs = import inputs.nixpkgs {
                system = mkGuestPlatform system;
                overlays = [
                  self.inputs.fenix.overlays.default
                  self.overlays.craneLib
                  self.overlays.blutgang
                ];
              };
            in
            pkgs.dockerTools.buildLayeredImage {
              name = blutgang.pname;
              tag = blutgang.version;
              contents = with pkgs; [
                pkg-config
                openssl
                blutgang
              ];
              config = {
                Workdir = "/app";
                Entrypoint = [ "${blutgang}/bin/blutgang" ];
              };
            };
        in
        {
          checks = {
            inherit blutgang;

            cargo-clippy = craneLib.cargoClippy (buildArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            cargo-doc = craneLib.cargoDoc (buildArgs // {
              inherit cargoArtifacts;
            });

            cargo-fmt = craneLibNightly.cargoFmt {
              inherit src;
            };

            taplo-fmt = craneLib.taploFmt {
              src = sources.sourceFilesBySuffices src [ ".toml" ];
            };

            cargo-audit = craneLib.cargoAudit {
              inherit src;
              inherit (inputs) advisory-db;
            };

            cargo-deny = craneLib.cargoDeny {
              inherit src;
            };

            cargo-nextest = craneLib.cargoNextest (buildArgs // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
              cargoNextestPartitionsExtraArgs = "--no-tests=warn";
            });

            # For checking metrics. On MacOS, requires nix-darwin linux-builder.
            #
            # `nix run .#checks.<system>.metrics-vm`
            metrics-vm = (inputs.nixpkgs.lib.nixosSystem {
              system = null;
              modules = [
                {
                  networking.hostName = "blutgang-metrics";
                  virtualisation.host.pkgs = pkgs;
                  nixpkgs = {
                    hostPlatform = mkGuestPlatform pkgs.stdenv.hostPlatform.system;
                    overlays = [
                      self.inputs.fenix.overlays.default
                      self.overlays.craneLib
                      self.overlays.blutgang
                    ];
                  };
                  environment.sessionVariables = {
                    RUST_LOG = "debug";
                  };
                  services.blutgang = {
                    enable = true;
                    settings.rpc = [{
                      url = "https://eth.merkle.io";
                      max_consecutive = 150;
                      max_per_second = 200;
                    }];
                  };
                }
                ./nixos/modules/metrics
                ./nixos/modules/virtualisation.nix
                ./nixos/modules/users.nix
                self.nixosModules.default
              ];
            }).config.system.build.vm;
          } // optionalAttrs (!pkgs.stdenv.isDarwin) {
            llvm-coverage = craneLibLlvm.cargoLlvmCov (buildArgs // {
              inherit cargoArtifacts;
            });

            # For debugging, this can be ran interactively:
            #
            # `nix run .#checks.<arch>-linux.nixos-module.driverInteractive`
            nixos-module = pkgs.nixosTest {
              name = "nixos-module";
              nodes.machine = { pkgs, ... }: {
                imports = [ self.nixosModules.default ];

                environment.sessionVariables = {
                  RUST_LOG = "debug";
                };

                services.blutgang = {
                  enable = true;
                  settings.rpc = [{
                    url = "https://eth.merkle.io";
                    max_consecutive = 150;
                    max_per_second = 200;
                  }];
                };
              };
              testScript = ''
                machine.start()
                machine.wait_for_unit("blutgang.service")
                machine.succeed("systemctl is-active blutgang.service | grep active")
                machine.fail("journalctl -u blutgang.service | grep -iE 'err(or)?'")
              '';
            };
          };

          packages = {
            inherit
              blutgang
              blutgang-systemd
              dockerImage
              ;
            default = blutgang;
          };

          devShells.default = craneLibNightly.devShell {
            checks = self.checks.${system}; # Inherit dev-tools from checks

            buildInputs = with pkgs; [
              cargo-flamegraph
              (python3.withPackages (pypkgs: with pypkgs; [
                requests
                websocket-client
              ]))
              nixpkgs-fmt
            ] ++ optional stdenv.isLinux [
              nil
              cargo-llvm-cov
              llvm_18
              valgrind
              gdb
            ];

            # Used by build.rs in the rocksdb-sys crate. If we don't set these, it would
            # try to build RocksDB from source.
            ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";

            # sccache stuff
            CARGO_BUILD_RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
            RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";

            # https://github.com/nix-community/fenix/issues/206
            # Tells rust which linker to use, in this case bintools, containing lld.
            # The problem doesn't seem to exist on MacOS.
            env = optionalAttrs (system == "x86_64-linux") {
              RUSTFLAGS = "-C link-self-contained=-linker";
            };

            # For generating code coverage reports using `cargo-llvm-cov`
            LLVM_COV = "${pkgs.llvm_18}/bin/llvm-cov";
            LLVM_PROFDATA = "${pkgs.llvm_18}/bin/llvm-profdata";

            # `jemalloc-sys` build step fails during debug builds due to nix gcc hardening
            hardeningDisable = [ "fortify" ];
          };

          formatter = pkgs.nixpkgs-fmt;
        }) // {
      overlays = {
        blutgang = import ./nixos/overlays;
        craneLib = final: prev: {
          craneLib = (self.inputs.crane.mkLib prev).overrideToolchain prev.fenix.stable.toolchain;
        };
      };

      nixosModules.default = { pkgs, config, lib, ... }: {
        imports = [ ./nixos/modules ];

        config.services.blutgang = {
          package = lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.blutgang-systemd;
        };
      };
    };
}
