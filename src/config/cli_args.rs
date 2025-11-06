//! Command line options for configuring blutgang.
//!
//! The configuration options take precedence in this order: command line, config file, defaults.
//! If a config file is present, but command line options are also present, the command line
//! options will override the config file options. If no config file is present, the default
//! configuration will be used. Following this pattern, command line options should not require
//! one another to be passed, since some options may be configured via `config.toml` or defaults.

use clap::builder::styling;

use crate::rpc::types::Rpc;

/// The terminal output style configuration.
pub const TERM_STYLE: styling::Styles = styling::Styles::styled()
    .header(styling::AnsiColor::Green.on_default().bold())
    .usage(styling::AnsiColor::Green.on_default().bold())
    .literal(styling::AnsiColor::Cyan.on_default())
    .placeholder(styling::AnsiColor::Cyan.on_default())
    .valid(styling::AnsiColor::Cyan.on_default());

// Help Headers
const CORE_OPTS: &str = "Core Configuration Options";
const RPC_OPTS: &str = "RPC Endpoint Options";
const CACHE_OPTS: &str = "Cache Options";
const ADMIN_OPTS: &str = "Admin Namespace Options";

// TODO: @eureka-cpu -- Add environment variables, and include a way to configure the metrics port?
#[derive(Debug, clap::Parser)]
#[command(
    name = "blutgang",
    version = crate::config::system::VERSION_STR,
    author,
    about = "Blutgang load balancer and cache. For more info read the wiki: https://github.com/rainshowerLabs/blutgang/wiki",
)]
pub struct Blutgang {
    // -- Core Configuration Options
    //
    /// Path to a TOML config file for blutgang.
    #[arg(long, short = 'c', help_heading = CORE_OPTS)]
    pub config: Option<std::path::PathBuf>,

    /// Specify RPC endpoints
    #[command(flatten)]
    pub rpc_list: RpcList,

    /// Address to listen to.
    #[arg(long, short = 'a', help_heading = CORE_OPTS)]
    pub address: Option<String>,

    /// Port to listen to.
    #[arg(long, short = 'p', help_heading = CORE_OPTS)]
    pub port: Option<u16>,

    /// Latency moving average length.
    #[arg(long, help_heading = CORE_OPTS)]
    pub ma_length: Option<f64>,

    /// Time for the RPC to respond before we remove it from the active queue.
    #[arg(long, help_heading = CORE_OPTS)]
    pub ttl: Option<u128>,

    /// Maximum amount of retries before we drop the current request.
    #[arg(long, help_heading = CORE_OPTS)]
    pub max_retries: Option<u32>,

    /// Block time in ms.
    #[arg(long, help_heading = CORE_OPTS)]
    pub expected_block_time: Option<u64>,

    /// How often to perform the health check.
    #[arg(long, help_heading = CORE_OPTS)]
    pub health_check_ttl: Option<u64>,

    /// Clear cache.
    #[arg(long, help_heading = CORE_OPTS)]
    pub clear_cache: bool,
    #[arg(long, hide = true, conflicts_with = "clear_cache")]
    pub no_clear_cache: bool,

    /// Sort RPCs by latency on startup.
    #[arg(long, help_heading = CORE_OPTS)]
    pub sort_on_startup: bool,
    #[arg(long, hide = true, conflicts_with = "sort_on_startup")]
    pub no_sort_on_startup: bool,

    /// Enable health checking.
    #[arg(long, help_heading = CORE_OPTS)]
    pub health_check: bool,
    #[arg(long, hide = true, conflicts_with = "health_check")]
    pub no_health_check: bool,

    /// Enable content type header checking. Useful if you want
    /// Blutgang to be JSON-RPC compliant.
    #[arg(long, help_heading = CORE_OPTS)]
    pub header_check: bool,
    #[arg(long, hide = true, conflicts_with = "header_check")]
    pub no_header_check: bool,

    /// Supress the RPC health check messages.
    #[arg(long, help_heading = CORE_OPTS)]
    pub supress_rpc_check: bool,
    #[arg(long, hide = true, conflicts_with = "supress_rpc_check")]
    pub no_supress_rpc_check: bool,

    // -- Cache Options
    //
    /// Enable a database backend.
    #[arg(long, short = 'D', help_heading = CACHE_OPTS)]
    pub db: Option<Db>,

    // -- Admin Namespace Options
    //
    /// Path to a privileged admin config.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_path: Option<std::path::PathBuf>,

    /// Enable the admin namespace.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin: bool,
    #[arg(long, hide = true, conflicts_with = "admin")]
    pub no_admin: bool,

    /// Address to listen to for the admin namespace.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_address: Option<String>,

    /// Port to listen to for the admin namespace.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_port: Option<u16>,

    /// Make the admin namespace readonly.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_readonly: bool,
    #[arg(long, hide = true, conflicts_with = "admin_readonly")]
    pub no_admin_readonly: bool,

    /// Enable admin comms with JWT.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_jwt: bool,
    #[arg(long, hide = true, conflicts_with_all = ["admin_key", "admin_jwt"])]
    pub no_admin_jwt: bool,

    /// JWT token.
    #[arg(long, help_heading = ADMIN_OPTS)]
    pub admin_key: Option<String>,
}

#[derive(Debug, clap::Args, Clone)]
pub struct RpcList {
    /// RPC endpoint [http(s)://]
    #[arg(long, help_heading = RPC_OPTS)]
    pub url: Vec<url::Url>,

    /// RPC endpoint [ws(s)://]
    #[arg(long, help_heading = RPC_OPTS)]
    pub ws_url: Vec<url::Url>,

    /// The maximum amount of time we can use this rpc in a row.
    #[arg(long, help_heading = RPC_OPTS)]
    pub max_consecutive: Vec<u32>,

    /// Max amount of queries per second.
    #[arg(long, help_heading = RPC_OPTS)]
    pub max_per_second: Vec<u64>,
}
impl RpcList {
    pub fn is_empty(&self) -> bool {
        self.url.is_empty()
    }
    pub fn into_rpcs(self, ma_length: f64) -> Vec<Rpc> {
        let RpcList {
            url,
            ws_url,
            max_consecutive,
            max_per_second,
        } = self;
        url.into_iter()
            .enumerate()
            .map(|(i, url)| {
                let mut delta = max_per_second.get(i).copied().unwrap_or(200);
                if delta != 0 {
                    delta = 1_000_000 / delta;
                }
                Rpc::new(
                    url,
                    ws_url.get(i).cloned(),
                    max_consecutive.get(i).copied().unwrap_or(150),
                    delta.into(),
                    ma_length,
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
pub(crate) enum Db {
    #[default]
    Sled,

    #[clap(name = "rocksdb")]
    RocksDb,
}
