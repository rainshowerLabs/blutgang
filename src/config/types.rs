use crate::{
    config::{
        cli_args::{
            self,
            Blutgang,
            TERM_STYLE,
        },
        error::ConfigError,
        setup::sort_by_latency,
        types::{
            rocksdb_config::RocksDbOptionsRepr,
            sled_config::SledConfigRepr,
        },
    },
    Rpc,
};
use clap::{
    ArgMatches,
    CommandFactory,
    FromArgMatches,
    ValueEnum,
};
use jsonwebtoken::DecodingKey;

use std::{
    fmt::{
        self,
        Debug,
    },
    net::SocketAddr,
};

use toml::Value;

pub(crate) mod rocksdb_config;
pub(crate) mod sled_config;

#[derive(Clone)]
pub struct AdminSettings {
    pub enabled: bool,
    pub address: SocketAddr,
    pub readonly: bool,
    pub jwt: bool,
    pub key: DecodingKey,
}

impl Default for AdminSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            address: "127.0.0.1:3001".parse::<SocketAddr>().unwrap(),
            readonly: false,
            jwt: false,
            key: DecodingKey::from_secret(b""),
        }
    }
}

impl Debug for AdminSettings {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AdminSettings {{")?;
        write!(f, " enabled: {:?}", self.enabled)?;
        write!(f, ", address: {:?}", self.address)?;
        write!(f, ", readonly: {:?}", self.readonly)?;
        write!(f, ", jwt: HIDDEN",)?;
        write!(f, " }}")
    }
}

#[derive(Clone)]
pub enum CacheSettings {
    Sled(sled::Config),
    RocksDB(rocksdb::Options),
}

#[derive(Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub sort_on_startup: bool,
    pub ma_length: f64,
    pub poverty_list: Vec<Rpc>,
    pub is_ws: bool,
    pub do_clear: bool,
    pub address: SocketAddr,
    pub health_check: bool,
    pub header_check: bool,
    pub ttl: u128,
    pub expected_block_time: u64,
    pub supress_rpc_check: bool,
    pub max_retries: u32,
    pub health_check_ttl: u64,
    pub cache: CacheSettings,
    pub admin: AdminSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rpc_list: Vec::new(),
            sort_on_startup: false,
            ma_length: 100.0,
            poverty_list: Vec::new(),
            is_ws: true,
            do_clear: false,
            address: "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
            health_check: false,
            header_check: true,
            ttl: 1000,
            expected_block_time: 12500,
            supress_rpc_check: true,
            max_retries: 32,
            health_check_ttl: 1000,
            cache: CacheSettings::Sled(sled::Config::default()),
            admin: AdminSettings::default(),
        }
    }
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        Self::try_parse(|| Blutgang::command().styles(TERM_STYLE).get_matches())
    }

    /// Use update syntax to handle sorting RPCs on startup. This avoids doing async work
    /// while parsing the configuration, deferring to the main thread before starting.
    pub(crate) async fn sort_on_startup(self) -> Result<Self, ConfigError> {
        tracing::info!("Sorting RPCs by latency...");
        let len = self.rpc_list.len();
        let (rpc_list, poverty_list) =
            sort_by_latency(self.rpc_list, Vec::with_capacity(len), self.ma_length).await?;

        Ok(Self {
            rpc_list,
            poverty_list,
            ..self
        })
    }

    // TODO: @eureka-cpu -- break this out into separate functions
    //
    /// Attempts to parse the available options from the config, applying command line options as overrides,
    /// otherwise falling back on default options.
    pub(crate) fn try_parse(matches: impl FnOnce() -> ArgMatches) -> Result<Self, ConfigError> {
        let args =
            Blutgang::from_arg_matches(&matches()).expect("failed to parse command line args");

        let mut settings = Self::default();

        let spanned_config = if let Some(config_path) = args
            .config
            .or_else(|| std::fs::canonicalize("./config.toml").ok())
        {
            let config_str = std::fs::read_to_string(&config_path).map_err(|err| {
                ConfigError::ReadError {
                    config: config_path.clone(),
                    err,
                }
            })?;
            Some(
                config_str
                    .parse::<Value>()
                    .map(|value| toml::Spanned::new(0..config_str.len(), value))
                    .map_err(|err| {
                        ConfigError::FailedDeserialization {
                            config: config_path,
                            err,
                        }
                    })?,
            )
        } else {
            None
        };
        let config = spanned_config.map(|spanned| spanned.into_inner());

        let blutgang = config
            .as_ref()
            .and_then(|config| config.get("blutgang"))
            .and_then(|blutgang| blutgang.as_table());

        // Get the db type from the command line args, or the config, otherwise use default.
        // Parse the config options for the db, otherwise use default.
        match args
            .db
            .or_else(|| {
                blutgang.and_then(|blutgang| {
                    blutgang.get("db").and_then(|db| {
                        db.as_str()
                            .and_then(|db| cli_args::Db::from_str(db, true).ok())
                    })
                })
            })
            .unwrap_or_default()
        {
            cli_args::Db::Sled => {
                let sled_config: SledConfigRepr = blutgang
                    .and_then(|blutgang| blutgang.get("sled"))
                    .and_then(|config| config.clone().try_into().ok())
                    .flatten()
                    .unwrap_or_default();

                settings.cache = CacheSettings::Sled(sled_config.into());
            }
            cli_args::Db::RocksDb => {
                let rocksdb_config: RocksDbOptionsRepr = blutgang
                    .and_then(|blutgang| blutgang.get("rocksdb"))
                    .and_then(|config| config.clone().try_into().ok())
                    .flatten()
                    .unwrap_or_default();

                settings.cache = CacheSettings::RocksDB(rocksdb_config.into());
            }
        }

        let mut is_ws = true;

        let address = args.address.or(blutgang.and_then(|blutgang| {
            blutgang
                .get("address")
                .and_then(|address| address.as_str().map(ToString::to_string))
        }));
        let port = args.port.or(blutgang.and_then(|blutgang| {
            blutgang.get("port").and_then(|port| {
                port.as_integer().map(|port| {
                    port.try_into()
                        .expect("failed to convert `port` into `u16`")
                })
            })
        }));
        if let Some((addr, port)) = address.zip(port) {
            settings.address = format!("{addr}:{port}")
                .parse::<SocketAddr>()
                .expect("failed to parse socket address");
        }

        if let Some(ma_length) = args.ma_length.or(blutgang.and_then(|blutgang| {
            blutgang
                .get("ma_length")
                .and_then(|ma_length| ma_length.as_float())
        })) {
            settings.ma_length = ma_length;
        }

        if let Some(ttl) = args.ttl.or(blutgang.and_then(|blutgang| {
            blutgang.get("ttl").and_then(|ttl| {
                ttl.as_integer()
                    .map(|ttl| ttl.try_into().expect("failed to convert `ttl` into `u128`"))
            })
        })) {
            settings.ttl = ttl;
        }

        if let Some(max_retries) = args.max_retries.or(blutgang.and_then(|blutgang| {
            blutgang.get("max_retries").and_then(|max_retries| {
                max_retries.as_integer().map(|max_retries| {
                    max_retries
                        .try_into()
                        .expect("failed to convert `max_retries` into `u32`")
                })
            })
        })) {
            settings.max_retries = max_retries;
        }

        if let Some(mut expected_block_time) =
            args.expected_block_time.or(blutgang.and_then(|blutgang| {
                blutgang.get("expected_block_time").and_then(|ebt| {
                    ebt.as_integer().map(|ebt| {
                        ebt.try_into()
                            .expect("failed to convert `expected_block_time` into `u64`")
                    })
                })
            }))
        {
            if expected_block_time == 0 {
                tracing::warn!("Expected_block_time is 0, turning off WS and health checks!");
                is_ws = false;
            } else {
                // This is to account for block propagation/execution/whatever delay
                expected_block_time = (expected_block_time as f64 * 1.1) as u64;
            }

            settings.expected_block_time = expected_block_time;
        }

        if let Some(health_check_ttl) = args.health_check_ttl.or(blutgang.and_then(|blutgang| {
            blutgang.get("health_check_ttl").and_then(|hcttl| {
                hcttl.as_integer().map(|hcttl| {
                    hcttl
                        .try_into()
                        .expect("failed to convert `health_check_ttl` into `u64`")
                })
            })
        })) {
            settings.health_check_ttl = health_check_ttl;
        }

        if args.clear_cache {
            settings.do_clear = args.clear_cache;
        } else if args.no_clear_cache {
            settings.do_clear = args.no_clear_cache;
        } else if let Some(clear_cache) = blutgang.and_then(|blutgang| {
            blutgang
                .get("clear_cache")
                .and_then(|clear_cache| clear_cache.as_bool())
        }) {
            settings.do_clear = clear_cache;
        }

        if args.sort_on_startup {
            settings.sort_on_startup = args.sort_on_startup;
        } else if args.no_sort_on_startup {
            settings.sort_on_startup = args.no_sort_on_startup;
        } else if let Some(sort_on_startup) = blutgang.and_then(|blutgang| {
            blutgang
                .get("sort_on_startup")
                .and_then(|sort| sort.as_bool())
        }) {
            settings.sort_on_startup = sort_on_startup;
        }

        if args.health_check {
            settings.health_check = args.health_check;
        } else if args.no_health_check {
            settings.health_check = args.no_health_check;
        } else if let Some(health_check) = blutgang.and_then(|blutgang| {
            blutgang
                .get("health_check")
                .and_then(|health_check| health_check.as_bool())
        }) {
            settings.health_check = health_check;
        }

        if args.header_check {
            settings.header_check = args.header_check;
        } else if args.no_header_check {
            settings.header_check = args.no_header_check;
        } else if let Some(header_check) = blutgang.and_then(|blutgang| {
            blutgang
                .get("header_check")
                .and_then(|header_check| header_check.as_bool())
        }) {
            settings.header_check = header_check;
        }

        if args.supress_rpc_check {
            settings.supress_rpc_check = args.supress_rpc_check;
        } else if args.no_supress_rpc_check {
            settings.supress_rpc_check = args.no_supress_rpc_check;
        } else if let Some(supress_rpc_check) = blutgang.and_then(|blutgang| {
            blutgang
                .get("supress_rpc_check")
                .and_then(|supress| supress.as_bool())
        }) {
            settings.supress_rpc_check = supress_rpc_check;
        }

        // TODO: @eureka-cpu -- parse admin.toml
        let admin_table =
            blutgang.and_then(|blutgang| blutgang.get("admin").and_then(|admin| admin.as_table()));
        let enabled = (args.admin)
            .then_some(args.admin)
            .or((args.no_admin).then_some(args.no_admin))
            .or(admin_table.and_then(|admin_table| {
                admin_table
                    .get("enable")
                    .and_then(|enable| enable.as_bool())
            }))
            .unwrap_or_default();
        if enabled {
            let mut admin_settings = AdminSettings::default();

            let address = args.admin_address.or(admin_table.and_then(|admin_table| {
                admin_table
                    .get("address")
                    .and_then(|address| address.as_str().map(ToString::to_string))
            }));
            let port = args.admin_port.or(admin_table.and_then(|admin_table| {
                admin_table.get("port").and_then(|port| {
                    port.as_integer()
                        .map(|i| i.try_into().expect("failed to parse admin port into `u16`"))
                })
            }));
            if let Some((addr, port)) = address.zip(port) {
                admin_settings.address = format!("{addr}:{port}")
                    .parse::<SocketAddr>()
                    .expect("failed to parse socket address");
            }

            if let Some(readonly) = (args.admin_readonly)
                .then_some(args.admin_readonly)
                .or((args.no_admin_readonly).then_some(args.no_admin_readonly))
                .or(admin_table.and_then(|admin_table| {
                    admin_table
                        .get("readonly")
                        .and_then(|readonly| readonly.as_bool())
                }))
            {
                admin_settings.readonly = readonly;
            }
            if let Some(jwt) = (args.admin_jwt)
                .then_some(args.admin_jwt)
                .or((args.no_admin_jwt).then_some(args.no_admin_jwt))
                .or(admin_table
                    .and_then(|admin_table| admin_table.get("jwt").and_then(|jwt| jwt.as_bool())))
            {
                admin_settings.jwt = jwt;
                if jwt {
                    admin_settings.key = DecodingKey::from_secret(
                        (args.admin_key)
                            .or(admin_table.and_then(|admin_table| {
                                admin_table
                                    .get("key")
                                    .and_then(|key| key.as_str().map(ToString::to_string))
                            }))
                            .expect("jwt is set but no key was found")
                            .as_bytes(),
                    );
                }
            }

            settings.admin = admin_settings;
        }

        if let Some(rpc_list) = (!args.rpc_list.is_empty())
            .then_some(args.rpc_list.into_rpcs(settings.ma_length))
            .or(config
                .as_ref()
                .and_then(|config| config.get("rpc"))
                .and_then(|rpc_list| {
                    rpc_list.as_array().map(|rpc_list| {
                        rpc_list
                            .iter()
                            .map(|rpc| {
                                let url = rpc
                                    .get("url")
                                    .and_then(|url| {
                                        url.as_str()
                                            .map(|url| url.parse().expect("failed to parse url"))
                                    })
                                    .expect("rpc is missing a url");
                                let ws_url = rpc.get("ws_url").and_then(|ws_url| {
                                    ws_url.as_str().map(|ws_url| {
                                        ws_url.parse().expect("failed to parse ws_url")
                                    })
                                });
                                let max_consecutive = rpc
                                    .get("max_consecutive")
                                    .and_then(|max_consec| {
                                        max_consec.as_integer().map(|i| {
                                            i.try_into().expect(
                                                "failed to parse `max_consecutive` into `u32`",
                                            )
                                        })
                                    })
                                    .expect("rpc is missing field `max_consecutive`");
                                let mut delta: u64 = rpc
                                    .get("max_per_second")
                                    .and_then(|mps| {
                                        mps.as_integer().map(|i| {
                                            i.try_into().expect(
                                                "failed to convert `max_per_second` into `u64`",
                                            )
                                        })
                                    })
                                    .expect("rpc is missing field `max_per_second`");
                                if delta != 0 {
                                    delta = 1_000_000 / delta;
                                }
                                if ws_url.is_none() {
                                    is_ws = false;
                                }

                                Rpc::new(
                                    url,
                                    ws_url,
                                    max_consecutive,
                                    delta.into(),
                                    settings.ma_length,
                                )
                            })
                            .collect::<Vec<Rpc>>()
                    })
                }))
        {
            settings.rpc_list = rpc_list;
        }

        if !is_ws {
            tracing::warn!("WebSocket endpoints not present for all nodes, or newHeads_ttl is 0.");
            tracing::warn!("Disabling WS only-features. Please check docs for more info.");
        }
        settings.is_ws = is_ws;

        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::cli_args::Blutgang;
    use clap::{
        ArgMatches,
        CommandFactory,
    };

    fn config_path_str() -> String {
        let config_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example_config.toml");
        config_path.into_os_string().into_string().unwrap()
    }
    fn command(cli_opts: Vec<String>, use_config: bool) -> ArgMatches {
        let mut cmd = vec!["blutgang".to_string()];
        if use_config {
            cmd.extend(["-c".to_string(), config_path_str()]);
        }
        cmd.extend(cli_opts);
        Blutgang::command().get_matches_from(cmd)
    }

    #[test]
    fn test_example_config() {
        assert!(
            super::Settings::try_parse(|| command(vec![], true)).is_ok(),
            "failed to parse example_config.toml"
        );
    }

    #[test]
    fn test_default_config() {
        assert!(
            super::Settings::try_parse(|| command(vec![], false)).is_ok(),
            "failed to parse default config"
        );
    }

    #[test]
    fn test_cli_overrides() {
        let rpc_url = "https://example.com/";
        let settings = super::Settings::try_parse(|| {
            command(vec!["--url".to_string(), rpc_url.to_string()], true)
        });

        assert!(
            settings.is_ok(),
            "failed to apply overrides to example_config.toml"
        );

        let settings = settings.unwrap();

        assert_eq!(
            settings.rpc_list.first().unwrap().get_url().as_str(),
            rpc_url
        );
    }
}
