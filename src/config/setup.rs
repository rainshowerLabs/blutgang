use crate::config::types::Settings;
use crate::Rpc;

use clap::Command;

// Sets the cli args
pub fn set_args(matches: Command) -> Settings {
    let matches = matches.get_matches();

    let rpc_list: String = matches
        .get_one::<String>("rpc_list")
        .expect("Invalid rpc_list")
        .to_string();
    // turn the rpc_list into a csv vec
    let rpc_list: Vec<&str> = rpc_list.split(",").collect();
    let rpc_list: Vec<String> = rpc_list.iter().map(|rpc| rpc.to_string()).collect();
    // Make a list of Rpc structs
    let rpc_list: Vec<Rpc> = rpc_list
        .iter()
        .map(|rpc| Rpc::new(rpc.to_string()))
        .collect();

    let port = matches.get_one::<String>("port").expect("Invalid port");
    let db_path = matches.get_one::<String>("db").expect("Invalid db path");
    let clear = matches.get_occurrences::<String>("clear").is_some();

    let settings = Settings {
        rpc_list,
        port: port.parse::<u16>().unwrap(),
        db_path: db_path.to_string(),
        cache_capacity: 1_000_000_000,
        print_profile: true,
        flush_time: Some(1000),
        do_clear: clear,
    };

    settings
}
