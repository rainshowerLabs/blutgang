{ pkgs ? import <nixpkgs> {
  overlays = [
    (import (builtins.fetchTarball {
      url = "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
    }))
  ];
}}:

pkgs.mkShell {
  buildInputs = [
    (pkgs.rust-bin.nightly.latest.default.override { 
      extensions = [ "rust-src" "rustfmt-preview" ];
    })
    pkgs.gcc
    pkgs.rust-analyzer
    pkgs.pkg-config
    pkgs.openssl
  ];

  shellHook = ''
    export CARGO_BUILD_RUSTC_WRAPPER=$(which sccache)
    export RUSTC_WRAPPER=$(which sccache)
  '';
}
