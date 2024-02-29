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
      in {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "blutgang";
          version = "0.3.2";
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

          cargoBuildFlags = [ "--profile maxperf" ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            gcc
            pkg-config
            openssl
            systemd
            (rust-bin.stable.latest.default.override { 
              extensions = [ "rust-src" "rustfmt-preview" "rust-analyzer"];
            })
          ];

          shellHook = ''
            export CARGO_BUILD_RUSTC_WRAPPER=$(which sccache)
            export RUSTC_WRAPPER=$(which sccache)
          '';
        };
      }
    );
}
