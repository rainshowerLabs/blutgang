use crate::config::types::Settings;
use crate::Rpc;

use sled::Config;
use clap::Command;
use std::net::SocketAddr;

// Sets the cli args
pub fn set_args(matches: Command) -> Settings {
    let matches = matches.get_matches();

    // Build the rpc_list
    let rpc_list: String = matches
        .get_one::<String>("rpc_list")
        .expect("Invalid rpc_list")
        .to_string();
    // Turn the rpc_list into a csv vec
    let rpc_list: Vec<&str> = rpc_list.split(",").collect();
    let rpc_list: Vec<String> = rpc_list.iter().map(|rpc| rpc.to_string()).collect();
    // Make a list of Rpc structs
    let rpc_list: Vec<Rpc> = rpc_list
        .iter()
        .map(|rpc| Rpc::new(rpc.to_string()))
        .collect();

    // Build the SocketAddr
    let address = matches
        .get_one::<String>("address")
        .expect("Invalid address");
    let port = matches.get_one::<String>("port").expect("Invalid port");
    // If the address contains `:` dont concatanate the port and just pass the address
    let address = if address.contains(":") {
        address.to_string()
    } else {
        format!("{}:{}", address, port)
    };

    let address = address
        .parse::<SocketAddr>()
        .expect("Invalid address or port!");

    // DB options
    let db_path = matches.get_one::<String>("db").expect("Invalid db path");

    let cache_capacity = matches.get_one::<String>("cache_capacity").expect("Invalid cache_capacity");
    let cache_capacity = cache_capacity.parse::<u64>().expect("Invalid cache_capacity");

    let print_profile = matches.get_occurrences::<String>("print_profile").is_some();

    let flush_every_ms = matches.get_one::<String>("flush_every_ms").expect("Invalid flush_every_ms");
    let flush_every_ms = flush_every_ms.parse::<u64>().expect("Invalid flush_every_ms");

    let clear = matches.get_occurrences::<String>("clear").is_some();

    // Set config for sled
    let sled_config = Config::default()
        .path(db_path)
        .mode(sled::Mode::HighThroughput)
        .cache_capacity(cache_capacity)
        .print_profile_on_drop(print_profile)
        .flush_every_ms(Some(flush_every_ms));

    let settings = Settings {
        rpc_list,
        do_clear: clear,
        address: address,
        sled_config: sled_config,
    };

    settings
}
