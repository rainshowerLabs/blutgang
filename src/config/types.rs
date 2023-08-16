use crate::Rpc;
use clap::ArgMatches;
use clap::Command;
use sled::Config;
use std::fs::{
    self,
};
use std::net::SocketAddr;
use toml::Value;

#[derive(Debug, Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub do_clear: bool,
    pub address: SocketAddr,
    pub ma_lenght: f64,
    pub sled_config: Config,
}

impl Settings {
    pub fn new(matches: Command) -> Settings {
        let matches = matches.get_matches();

        // Try to open the file at the path specified in the args
        let path = matches.get_one::<String>("config").unwrap();
        let file: Option<String> = match fs::read_to_string(&path) {
            Ok(file) => Some(file),
            Err(_) => None,
        };

        if file.is_some() {
            println!("Using config file at {}...", path);
            return Settings::create_from_file(file.unwrap());
        }
        println!("Using command line arguments for settings...");
        Settings::create_from_matches(matches)
    }

    fn create_from_file(conf_file: String) -> Settings {
        let parsed_toml = conf_file.parse::<Value>().expect("Error parsing TOML");

        let table_names: Vec<&String> = parsed_toml.as_table().unwrap().keys().collect::<Vec<_>>();

        // Parse the `blutgang` table
        let blutgang_table = parsed_toml.get("blutgang").unwrap().as_table().unwrap();
        let do_clear = blutgang_table.get("do_clear").unwrap().as_bool().unwrap();
        let address = blutgang_table.get("address").unwrap().as_str().unwrap();

        // Build the SocketAddr
        let port = 3000;
        // If the address contains `:` dont concatanate the port and just pass the address
        let address = if address.contains(":") {
            address.to_string()
        } else {
            format!("{}:{}", address, port)
        };
        let address = address.parse::<SocketAddr>().unwrap();

        let ma_lenght = blutgang_table.get("ma_lenght").unwrap().as_float().unwrap();

        // Parse `sled` table
        let sled_table = parsed_toml.get("sled").unwrap().as_table().unwrap();
        let db_path = sled_table.get("db_path").unwrap().as_str().unwrap();
        let cache_capacity = sled_table.get("cache_capacity").unwrap().as_str().unwrap();
        let compression = sled_table.get("compression").unwrap().as_bool().unwrap();
        let print_profile = sled_table.get("print_profile").unwrap().as_bool().unwrap();
        let flush_every_ms = sled_table
            .get("flush_every_ms")
            .unwrap()
            .as_float()
            .unwrap();

        // Create sled config
        let sled_config = Config::new()
            .path(db_path)
            .cache_capacity(cache_capacity.parse::<usize>().unwrap().try_into().unwrap())
            .compression_factor(0)
            .flush_every_ms(Some(flush_every_ms as u64))
            .print_profile_on_drop(print_profile)
            .use_compression(compression);

        // Parse all the other tables as RPCs and put them in a Vec<Rpc>
        let mut rpc_list: Vec<Rpc> = Vec::new();
        for table_name in table_names {
            if table_name != "blutgang" && table_name != "sled" {
                let rpc_table = parsed_toml.get(table_name).unwrap().as_table().unwrap();
                let rpc = Rpc::new(rpc_table.get("url").unwrap().as_str().unwrap().to_string());
                rpc_list.push(rpc);
            }
        }
        Settings {
            rpc_list,
            do_clear: do_clear,
            address: address,
            ma_lenght: ma_lenght,
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

        let ma_lenght = matches
            .get_one::<String>("ma_lenght")
            .expect("Invalid ma_lenght");
        let ma_lenght = ma_lenght.parse::<f64>().expect("Invalid ma_lenght");

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
            ma_lenght: ma_lenght,
            sled_config: sled_config,
        }
    }
}
