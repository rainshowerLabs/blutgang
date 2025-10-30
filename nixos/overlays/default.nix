# TODO: @eureka-cpu -- we could maybe deduplicate stuff by just using the overlay
final: prev:

let
  inherit (prev) craneLib;
  inherit (prev.lib) optionalAttrs optional;
  inherit (prev.lib.fileset) unions fileFilter toSource;

  src =
    let
      root = ../../.;
      fileset = unions [
        (craneLib.fileset.commonCargoSources root)
        (fileFilter (file: file.hasExt "md") root)
        (root + "/LICENSE")
      ];
    in
    toSource {
      inherit root fileset;
    };

  buildArgs = {
    inherit src;
    strictDeps = true;

    nativeBuildInputs = with final; [
      pkg-config
      rustPlatform.bindgenHook
      llvmPackages.bintools
      libclang.lib
    ];
    buildInputs = with final; [
      openssl.dev
      rocksdb
    ];

    # Used by build.rs in the rocksdb-sys crate. If we don't set these, it would
    # try to build RocksDB from source.
    ROCKSDB_LIB_DIR = "${final.rocksdb}/lib";
    # https://github.com/nix-community/fenix/issues/206
    # Tells rust which linker to use, in this case bintools, containing lld.
    # The problem doesn't seem to exist on MacOS.
    env = optionalAttrs (final.stdenv.buildPlatform.system == "x86_64-linux") {
      RUSTFLAGS = "-C link-self-contained=-linker";
    };
  };

  # Build *just* the cargo dependencies, so we can reuse
  # all of that work (e.g. via cachix) when running in CI
  cargoArtifacts = craneLib.buildDepsOnly buildArgs;

  mkBlutgang = { features ? [ ] }: craneLib.buildPackage (buildArgs // {
    inherit cargoArtifacts;

    # Only patchelf for the actual package
    nativeBuildInputs = with final; buildArgs.nativeBuildInputs
      ++ optional stdenv.isLinux [
      autoPatchelfHook
    ];
    doCheck = false;

    CARGO_PROFILE = "maxperf";
  } // optionalAttrs (builtins.length features > 0) {
    cargoExtraArgs = "--locked --features ${builtins.concatStringsSep " " features}";
  });
in
{
  blutgang = mkBlutgang { };
  blutgang-systemd = mkBlutgang { features = [ "journald" ]; };
}
