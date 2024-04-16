use crate::config::system::VERSION_STR;

use clap::{
    Arg,
    Command,
};

pub fn create_match() -> clap::Command {
    Command::new("blutgang")
        .version(VERSION_STR)
        .author("makemake <vukasin@gostovic.me> and contributors")
        .about("Blutgang load balancer and cache. For more info read the wiki: https://github.com/rainshowerLabs/blutgang/wiki")
        .arg(Arg::new("rpc_list")
            .long("rpc_list")
            .short('r')
            .num_args(1..)
            .default_value("")
            .conflicts_with("config")
            .help("CSV list of rpcs"))
        .arg(Arg::new("config")
            .long("config")
            .short('c')
            .num_args(1..)
            .default_value("config.toml")
            .conflicts_with("rpc_list")
            .help("TOML config file for blutgang"))
        .arg(Arg::new("port")
            .long("port")
            .short('p')
            .num_args(1..)
            .default_value("3000")
            .help("port to listen to"))
        .arg(Arg::new("address")
            .long("address")
            .short('a')
            .num_args(1..)
            .default_value("127.0.0.1")
            .help("port to listen to"))
        .arg(Arg::new("ma_length")
            .long("ma_length")
            .num_args(1..)
            .default_value("15")
            .help("Latency moving average length"))
        .arg(Arg::new("db")
            .long("db")
            .short('d')
            .num_args(1..)
            .default_value("blutgang-cache")
            .help("Database path"))
        .arg(Arg::new("cache_capacity")
            .long("cache_capacity")
            .num_args(1..)
            .default_value("1000000000")
            .help("Capacity of the cache stored in memory in bytes"))
        .arg(Arg::new("print_profile")
            .long("print_profile")
            .num_args(0..)
            .help("Print DB profile on drop"))
        .arg(Arg::new("compression")
            .long("compression")
            .num_args(0..)
            .help("Use zstd compression"))
        .arg(Arg::new("flush_every_ms")
            .long("flush_every_ms")
            .num_args(1..)
            .default_value("1000")
            .help("Time in ms to flush the DB"))
        .arg(Arg::new("clear")
            .long("clear")
            .num_args(0..)
            .help("Clear cache"))
        .arg(Arg::new("")
            .long("health_check")
            .num_args(0..)
            .help("Enable health checking"))
        .arg(Arg::new("ttl")
            .long("ttl")
            .num_args(1..)
            .default_value("300")
            .help("Time for the RPC to respond before we remove it from the active queue"))
        .arg(Arg::new("supress_rpc_check")
            .long("supress_rpc_check")
            .num_args(1..)
            .default_value("false")
            .help("Supress the checking RPC health messages"))
        .arg(Arg::new("max_retries")
            .long("max_retries")
            .num_args(1..)
            .default_value("32")
            .help("Maximum amount of retries before we drop the current request."))
        .arg(Arg::new("health_check_ttl")
            .long("health_check_ttl")
            .num_args(1..)
            .default_value("2000")
            .help("How often to perform the health check"))
        .arg(Arg::new("admin")
            .long("admin")
            .num_args(0..)
            .help("Enable the admin namespace"))
        .arg(Arg::new("admin_port")
            .long("admin_port")
            .num_args(1..)
            .default_value("5715")
            .help("Port to listen to for the admin namespace"))
        .arg(Arg::new("admin_address")
            .long("admin_address")
            .num_args(1..)
            .default_value("127.0.0.1")
            .help("Address to listen to for the admin namespace"))
        .arg(Arg::new("readonly")
            .long("readonly")
            .num_args(0..)
            .help("Make the admin namespace be readonly"))
        .arg(Arg::new("jwt")
            .long("jwt")
            .requires("token")
            .num_args(0..)
            .help("Enable admin comms with JWT"))
        .arg(Arg::new("token")
            .long("token")
            .num_args(1..)
            .requires("jwt")
            .help("JWT token"))
        .arg(Arg::new("metrics")
            .long("metrics")
            .num_args(0..)
            .help("Enable the metrics namespace"))
        .arg(Arg::new("metrics_port")
            .long("metrics_port")
            .num_args(1..)
            .default_value("9091")
            .help("Port to listen to for the metrics namespace"))
        .arg(Arg::new("metrics_address")
            .long("metrics_address")
            .num_args(1..)
            .default_value("127.0.0.1")
            .help("Address to listen to for the metrics namespace"))
}
