use crate::config::setup::sort_by_latency;
use crate::Rpc;
use clap::ArgMatches;
use clap::Command;
use sled::Config;
use std::fs::{self,};
use std::net::SocketAddr;
use toml::Value;

#[derive(Debug, Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub do_clear: bool,
    pub address: SocketAddr,
    pub ma_length: f64,
    pub sled_config: Config,
}

impl Settings {
    pub async fn new(matches: Command) -> Settings {
        let matches = matches.get_matches();

        // Try to open the file at the path specified in the args
        let path = matches.get_one::<String>("config").unwrap();
        let file: Option<String> = match fs::read_to_string(&path) {
            Ok(file) => Some(file),
            Err(_) => panic!("Error opening config file at {}", path),
        };

        if file.is_some() {
            println!("Using config file at {}...", path);
            return Settings::create_from_file(file.unwrap()).await;
        }
        println!("Using command line arguments for settings...");
        Settings::create_from_matches(matches)
    }

    async fn create_from_file(conf_file: String) -> Settings {
        let parsed_toml = conf_file.parse::<Value>().expect("Error parsing TOML");

        let table_names: Vec<&String> = parsed_toml.as_table().unwrap().keys().collect::<Vec<_>>();

        // Parse the `blutgang` table
        let blutgang_table = parsed_toml.get("blutgang").unwrap().as_table().unwrap();
        let do_clear = blutgang_table.get("do_clear").unwrap().as_bool().unwrap();
        let address = blutgang_table.get("address").unwrap().as_str().unwrap();
        let sort_on_startup = blutgang_table
            .get("sort_on_startup")
            .unwrap()
            .as_bool()
            .unwrap();

        // Build the SocketAddr
        let port = 3000;
        // Replace `localhost` if it exists
        let address = address.replace("localhost", "127.0.0.1");
        // If the address contains `:` dont concatanate the port and just pass the address
        let address = if address.contains(":") {
            address.to_string()
        } else {
            format!("{}:{}", address, port)
        };
        let address = address.parse::<SocketAddr>().unwrap();

        let ma_length = blutgang_table
            .get("ma_length")
            .unwrap()
            .as_integer()
            .unwrap() as f64;

        // Parse `sled` table
        let sled_table = parsed_toml.get("sled").unwrap().as_table().unwrap();
        let db_path = sled_table.get("db_path").unwrap().as_str().unwrap();
        let cache_capacity = sled_table
            .get("cache_capacity")
            .unwrap()
            .as_integer()
            .unwrap() as usize;
        let compression = sled_table.get("compression").unwrap().as_bool().unwrap();
        let print_profile = sled_table.get("print_profile").unwrap().as_bool().unwrap();
        let flush_every_ms = sled_table
            .get("flush_every_ms")
            .unwrap()
            .as_integer()
            .unwrap();

        // Parse sled mode
        let sled_mode_str = sled_table.get("mode").unwrap().as_str().unwrap();
        let mut sled_mode = sled::Mode::HighThroughput;

        if sled_mode_str == "LowSpace" {
            sled_mode = sled::Mode::LowSpace;
        }

        // Create sled config
        let sled_config = Config::new()
            .path(db_path)
            .cache_capacity(cache_capacity.try_into().unwrap())
            .mode(sled_mode)
            .flush_every_ms(Some(flush_every_ms as u64))
            .print_profile_on_drop(print_profile)
            .use_compression(compression);

        // Parse all the other tables as RPCs and put them in a Vec<Rpc>
        //
        // Sort RPCs by latency if enabled
        let mut rpc_list: Vec<Rpc> = Vec::new();
        for table_name in table_names {
            if table_name != "blutgang" && table_name != "sled" {
                let rpc_table = parsed_toml.get(table_name).unwrap().as_table().unwrap();

                let max_consecutive = rpc_table
                    .get("max_consecutive")
                    .unwrap()
                    .as_integer()
                    .unwrap() as u32;
                let url = rpc_table.get("url").unwrap().as_str().unwrap().to_string();

                let rpc = Rpc::new(url, max_consecutive);
                rpc_list.push(rpc);
            }
        }

        if sort_on_startup {
            println!("Sorting RPCs by latency...");
            rpc_list = sort_by_latency(rpc_list, ma_length).await;
        }

        Settings {
            rpc_list,
            do_clear: do_clear,
            address: address,
            ma_length: ma_length,
            sled_config: sled_config,
        }
    }

    fn create_from_matches(matches: ArgMatches) -> Settings {
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
            .map(|rpc| Rpc::new(rpc.to_string(), 6))
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

        let cache_capacity = matches
            .get_one::<String>("cache_capacity")
            .expect("Invalid cache_capacity");
        let cache_capacity = cache_capacity
            .parse::<u64>()
            .expect("Invalid cache_capacity");

        let print_profile = matches.get_occurrences::<String>("print_profile").is_some();

        let flush_every_ms = matches
            .get_one::<String>("flush_every_ms")
            .expect("Invalid flush_every_ms");
        let flush_every_ms = flush_every_ms
            .parse::<u64>()
            .expect("Invalid flush_every_ms");

        let ma_length = matches
            .get_one::<String>("ma_length")
            .expect("Invalid ma_length");
        let ma_length = ma_length.parse::<f64>().expect("Invalid ma_length");

        let clear = matches.get_occurrences::<String>("clear").is_some();
        let compression = matches.get_occurrences::<String>("compression").is_some();

        // Set config for sled
        let sled_config = Config::default()
            .path(db_path)
            .mode(sled::Mode::HighThroughput)
            .cache_capacity(cache_capacity)
            .use_compression(compression)
            .print_profile_on_drop(print_profile)
            .flush_every_ms(Some(flush_every_ms));

        Settings {
            rpc_list,
            do_clear: clear,
            address: address,
            ma_length: ma_length,
            sled_config: sled_config,
        }
    }
}
