{
  description = "Blutgang - The WD40 of Ethereum load balancers";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem [ "aarch64-linux" "aarch64-darwin" "x86_64-linux" ] (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        cargoMeta = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoMeta.package.name;
          version = cargoMeta.package.version;
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          buildInputs = with pkgs; [ gcc pkg-config openssl systemd ];
          nativeBuildInputs = with pkgs; [
            gcc
            pkg-config
            openssl
            systemd
            (rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rustfmt-preview" "rust-analyzer" ];
            })
          ];
          # This is the only way im aware of to have
          # different build profiles with `buildRustPackage` (:
          preBuild = ''
            # Backup the original Cargo.toml
            cp Cargo.toml Cargo.toml.backup
            # Add the desired profile settings to Cargo.toml
            echo '[profile.release]' >> Cargo.toml
            echo 'lto = "fat"' >> Cargo.toml
            echo 'codegen-units = 1' >> Cargo.toml
            echo 'incremental = false' >> Cargo.toml
          '';
          postBuild = ''
            # Restore the original Cargo.toml
            mv Cargo.toml.backup Cargo.toml
          '';
          cargoBuildFlags = [ "" ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            pkg-config
            openssl
            clang
            cargo-flamegraph
            python311Packages.requests
            python311Packages.websocket-client
            python3
            (rust-bin.nightly.latest.default.override {
              extensions = [ "rust-src" "rustfmt-preview" "rust-analyzer" ];
            })
          ] ++ lib.optional pkgs.stdenv.isLinux [
            pkgs.cargo-llvm-cov
            pkgs.llvm_18
            pkgs.valgrind
            pkgs.gdb
            pkgs.gcc
            pkgs.systemd
            pkgs.linuxPackages_latest.perf
          ] ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.SystemConfiguration
          ];

          # `jemalloc-sys` build step fails during debug builds due to nix gcc hardening
          hardeningDisable = ["fortify"];

          shellHook = ''
            export CARGO_BUILD_RUSTC_WRAPPER=$(which sccache)
            export RUSTC_WRAPPER=$(which sccache)
            export OLD_PS1="$PS1" # Preserve the original PS1
            export PS1="nix-shell:blutgang $PS1"

            # For generating code coverage reports using `cargo-llvm-cov`
            export LLVM_COV=/nix/store/smh2gh3sjmj51hrp3vrb6n3lsqda4w3l-llvm-18.1.7/bin/llvm-cov
            export LLVM_PROFDATA=/nix/store/smh2gh3sjmj51hrp3vrb6n3lsqda4w3l-llvm-18.1.7/bin/llvm-profdata
          '';

          # reset ps1
          shellExitHook = ''
            export PS1="$OLD_PS1"
          '';
        };
      }
    );
}
