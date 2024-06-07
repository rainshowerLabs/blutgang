{
  description = "Blutgang - The WD40 of Ethereum load balancers";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachSystem [ "aarch64-linux" "x86_64-linux" ] (system:
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
            gcc
            pkg-config
            openssl
            systemd
            clang
            gdb
            valgrind
            python311Packages.requests
            python311Packages.websocket-client
            (rust-bin.nightly.latest.default.override {
              extensions = [ "rust-src" "rustfmt-preview" "rust-analyzer" ];
            })
          ];

          shellHook = ''
            export CARGO_BUILD_RUSTC_WRAPPER=$(which sccache)
            export RUSTC_WRAPPER=$(which sccache)
            export OLD_PS1="$PS1" # Preserve the original PS1
            export PS1="nix-shell:blutgang $PS1" # Customize this line as needed

            # Set NIX_LD and NIX_LD_LIBRARY_PATH for rust-analyzer
            # export NIX_LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [ pkgs.glibc pkgs.gcc-unwrapped.lib ]}"
            # export NIX_LD="${pkgs.stdenv.cc}/nix-support/dynamic-linker"
          '';

          # reset ps1
          shellExitHook = ''
            export PS1="$OLD_PS1"
          '';
        };
      }
    );
}