{ config, lib, pkgs, ... }:
let
  inherit (lib)
    mkEnableOption
    mkPackageOption
    mkOption
    mkIf
    types
    mapAttrs
    filterAttrs
    toLower
    ;
  cfg = config.services.blutgang;
in
{
  options.services.blutgang = {
    enable = mkEnableOption "blutgang";

    package = mkPackageOption pkgs "blutgang" { };

    printConfig = mkOption {
      type = types.bool;
      default = false;
      description = "Print the `blutgang.toml` created from `settings` to stdout during evaluation.";
    };

    settings = {
      blutgang = {
        address = mkOption {
          type = types.nullOr types.str;
          default = null;
          description = "Address to bind blutgang to";
        };
        port = mkOption {
          type = types.nullOr types.port;
          default = 3000;
          description = "Port to bind blutgang to";
        };
        clear_cache = mkOption {
          type = types.nullOr types.bool;
          default = null;
          description = "Clear the cache DB on startup";
        };
        ma_length = mkOption {
          type = types.nullOr types.float;
          default = null;
          description = "Moving average length for the latency";
        };
        sort_on_startup = mkOption {
          type = types.nullOr types.bool;
          default = null;
          description = "Sort RPCs by latency on startup. Recommended to leave on.";
        };
        health_check = mkOption {
          type = types.nullOr types.bool;
          default = null;
          description = "Enable health checking";
        };
        header_check = mkOption {
          type = types.nullOr types.bool;
          default = null;
          description = ''
            Enable content type header checking. Set this to `true` if you want
            Blutgang to be JSON-RPC compliant.
          '';
        };
        ttl = mkOption {
          type = types.nullOr types.ints.unsigned;
          default = null;
          description = "Acceptable time to wait for a response in ms";
        };
        max_retries = mkOption {
          type = types.nullOr types.ints.unsigned;
          default = null;
          description = "How many times to retry a request before giving up";
        };
        expected_block_time = mkOption {
          type = types.nullOr types.ints.unsigned;
          default = null;
          description = "Block time in ms, used as a sanity check when not receiving subscriptions";
        };
        health_check_ttl = mkOption {
          type = types.nullOr types.ints.unsigned;
          default = null;
          description = "Time between health checks in ms";
        };
        supress_rpc_check = mkOption {
          type = types.nullOr types.bool;
          default = null;
          description = "Supress the health check running info messages";
        };
        db = mkOption {
          type = types.nullOr (types.enum [ "sled" "rocksdb" ]);
          default = null;
          description = "Choose which database backend to use for caching";
        };

        admin = {
          enable = mkOption {
            type = types.nullOr types.bool;
            default = null;
            description = ''
              Enable the admin RPC namespace.

              **Bindings for admin namespace settings are not available
              since it could expose secrets as world-readable via the nix store.**
            '';
          };
          path = mkOption {
            type = types.nullOr types.path;
            default = null;
            description = ''
              Path to a privileged admin config.

              **Bindings for admin namespace settings are not available
              since it could expose secrets as world-readable via the nix store.**
            '';
          };
        };

        sled = {
          path = mkOption {
            type = types.nullOr types.str;
            default = null;
            description = "Path to sled database";
          };
          cache_capacity = mkOption {
            type = types.nullOr types.ints.unsigned;
            default = null;
            description = "Cache size in bytes";
          };
          compression = mkOption {
            type = types.nullOr types.int;
            default = null;
            description = "zstd compression level";
          };
          flush_every_ms = mkOption {
            type = types.nullOr types.ints.unsigned;
            default = null;
            description = "Frequency of flushes in ms";
          };
        };

        rocksdb =
          let
            compression_type = types.enum [
              "none"
              "snappy"
              "zlib"
              "bz2"
              "lz4"
              "lz4hc"
              "zstd"
            ];
          in
          {
            create_if_missing = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            create_missing_column_families = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            error_if_exists = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            paranoid_checks = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            db_paths = mkOption {
              type = types.nullOr (types.listOf (types.submodule {
                options = {
                  path = mkOption { type = types.str; };
                  target_size = mkOption { type = types.ints.unsigned; };
                };
              }));
              default = null;
            };
            db_log_dir = mkOption {
              type = types.nullOr types.str;
              default = null;
            };
            wal_dir = mkOption {
              type = types.nullOr types.str;
              default = null;
            };
            increase_parallelism = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            optimize_level_style_compaction = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            optimize_universal_style_compaction = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            optimize_for_point_lookup = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            max_open_files = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            max_file_opening_threads = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            max_background_jobs = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            write_buffer_size = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            max_write_buffer_number = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            min_write_buffer_number = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            target_file_size_base = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            max_bytes_for_level_base = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            max_bytes_for_level_multiplier = mkOption {
              type = types.nullOr types.float;
              default = null;
            };
            level_zero_file_num_compaction_trigger = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            level_zero_slowdown_writes_trigger = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            level_zero_stop_writes_trigger = mkOption {
              type = types.nullOr types.int;
              default = null;
            };
            disable_auto_compactions = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            use_fsync = mkOption {
              type = types.nullOr types.bool;
              default = null;
            };
            bytes_per_sync = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            wal_bytes_per_sync = mkOption {
              type = types.nullOr types.ints.unsigned;
              default = null;
            };
            compression_type = mkOption {
              type = types.nullOr compression_type;
              default = null;
              apply = compression: if compression != null then toLower compression else compression;
            };
            bottommost_compression_type = mkOption {
              type = types.nullOr compression_type;
              default = null;
              apply = compression: if compression != null then toLower compression else compression;
            };
          };
      };

      rpc = mkOption {
        type = types.listOf (types.submodule {
          options = {
            url = mkOption {
              type = types.str;
              description = "The HTTP URL of the RPC endpoint";
            };
            ws_url = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "The websocket URL of the RPC endpoint (optional)";
            };
            max_consecutive = mkOption {
              type = types.ints.unsigned;
              description = "The maximum amount of time we can use this RPC in a row.";
            };
            max_per_second = mkOption {
              type = types.ints.unsigned;
              description = "Maximum number of queries per second for this RPC.";
            };
          };
        });
        description = "List of RPC endpoints for Blutgang";
      };
    };
  };

  config = mkIf cfg.enable {
    systemd.services.blutgang =
      let
        # `writeTOML` cannot handle conversion of `types.nullOr`, so we recursively filter them from the options before writing.
        filterNull = settings:
          if builtins.isAttrs settings then
            let
              filtered = filterAttrs (_: attr: attr != null) (mapAttrs (_: filterNull) settings);
            in
            if filtered == { } then null else filtered
          else if builtins.isList settings then
            map filterNull settings
          else
            settings;

        # Print the TOML created from settings to stdout for debugging.
        debug-toml = builtins.trace "${blutgang-toml}:\n\n${builtins.readFile blutgang-toml}" blutgang-toml;
        blutgang-toml = pkgs.writers.writeTOML "blutgang.toml" (filterNull cfg.settings);
      in
      {
        description = "blutgang";
        wantedBy = [ "multi-user.target" ];
        serviceConfig = {
          Type = "simple";
          ExecStart = "${cfg.package}/bin/blutgang -c ${if cfg.printConfig then debug-toml else blutgang-toml}";
          Restart = "on-failure";
          RestartSec = 2;
        };
      };
  };
}
