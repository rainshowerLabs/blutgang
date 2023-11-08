use crate::{
    config::setup::sort_by_latency,
    Rpc,
};
use clap::{
    ArgMatches,
    Command,
};
use jsonwebtoken::DecodingKey;

use sled::Config;

use std::{
    fmt,
    fmt::Debug,
    fs::{
        self,
    },
    net::SocketAddr,
};

use toml::Value;

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
            enabled: true,
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

#[derive(Debug, Clone)]
pub struct Settings {
    pub rpc_list: Vec<Rpc>,
    pub do_clear: bool,
    pub address: SocketAddr,
    pub health_check: bool,
    pub ttl: u128,
    pub health_check_ttl: u64,
    pub sled_config: Config,
    pub admin: AdminSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rpc_list: Vec::new(),
            do_clear: false,
            address: "127.0.0.1:3000".parse::<SocketAddr>().unwrap(),
            health_check: false,
            ttl: 1000,
            health_check_ttl: 1000,
            sled_config: sled::Config::default(),
            admin: AdminSettings::default(),
        }
    }
}

impl Settings {
    pub async fn new(matches: Command) -> Settings {
        let matches = matches.get_matches();

        // Try to open the file at the path specified in the args
        let path = matches.get_one::<String>("config").unwrap();
        let file: Option<String> = match fs::read_to_string(path) {
            Ok(file) => Some(file),
            Err(_) => panic!("\x1b[31mErr:\x1b[0m Error opening config file at {}", path),
        };

        if let Some(file) = file {
            println!("\x1b[35mInfo:\x1b[0m Using config file at {}", path);
            return Settings::create_from_file(file).await;
        }
        
        println!("\x1b[35mInfo:\x1b[0m Using command line arguments for settings...");
        Settings::create_from_matches(matches)
    }

    async fn create_from_file(conf_file: String) -> Settings {
        let parsed_toml = conf_file.parse::<Value>().expect("Error parsing TOML");

        let table_names: Vec<&String> = parsed_toml.as_table().unwrap().keys().collect::<Vec<_>>();

        // Parse the `blutgang` table
        let blutgang_table = parsed_toml
            .get("blutgang")
            .expect("\x1b[31mErr:\x1b[0m Missing blutgang table!")
            .as_table()
            .expect("\x1b[31mErr:\x1b[0m Could not parse blutgang table!");
        let do_clear = blutgang_table
            .get("do_clear")
            .expect("\x1b[31mErr:\x1b[0m Missing do_clear toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse do_clear as bool!");
        let address = blutgang_table
            .get("address")
            .expect("\x1b[31mErr:\x1b[0m Missing address!")
            .as_str()
            .expect("\x1b[31mErr:\x1b[0m Could not parse address as str!");
        let sort_on_startup = blutgang_table
            .get("sort_on_startup")
            .expect("\x1b[31mErr:\x1b[0m Missing sort_on_startup toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse sort_on_startup as bool!");

        // Build the SocketAddr
        let port = 3000;
        // Replace `localhost` if it exists
        let address = address.replace("localhost", "127.0.0.1");
        // If the address contains `:` dont concatanate the port and just pass the address
        let address = if address.contains(':') {
            address.to_string()
        } else {
            format!("{}:{}", address, port)
        };
        let address = address
            .parse::<SocketAddr>()
            .expect("\x1b[31mErr:\x1b[0m Could not address to SocketAddr!");

        let ma_length = blutgang_table
            .get("ma_length")
            .expect("\x1b[31mErr:\x1b[0m Missing ma_length!")
            .as_integer()
            .expect("\x1b[31mErr:\x1b[0m Could not parse ma_length as int!")
            as f64;

        let health_check = blutgang_table
            .get("health_check")
            .expect("\x1b[31mErr:\x1b[0m Missing health_check toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse health_check as bool!");
        let ttl = blutgang_table
            .get("ttl")
            .expect("\x1b[31mErr:\x1b[0m Missing ttl!")
            .as_integer()
            .expect("\x1b[31mErr:\x1b[0m Could not parse ttl as int!") as u128;

        let health_check_ttl = if health_check {
            blutgang_table
                .get("health_check_ttl")
                .expect("\x1b[31mErr:\x1b[0m Missing health_check_ttl!")
                .as_integer()
                .expect("\x1b[31mErr:\x1b[0m Could not parse health_check_ttl as int!")
                as u64
        } else {
            u64::MAX
        };

        // Parse `sled` table
        let sled_table = parsed_toml
            .get("sled")
            .expect("\x1b[31mErr:\x1b[0m Missing sled table!")
            .as_table()
            .expect("\x1b[31mErr:\x1b[0m Could not parse sled_table as table!");
        let db_path = sled_table
            .get("db_path")
            .expect("\x1b[31mErr:\x1b[0m Missing db_path!")
            .as_str()
            .expect("\x1b[31mErr:\x1b[0m Could not parse db_path as str!");
        let cache_capacity = sled_table
            .get("cache_capacity")
            .expect("\x1b[31mErr:\x1b[0m Missing cache_capacity!")
            .as_integer()
            .expect("\x1b[31mErr:\x1b[0m Could not parse cache_capacity as int!")
            as usize;
        let compression = sled_table
            .get("compression")
            .expect("\x1b[31mErr:\x1b[0m Missing compression toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse compression as bool!");
        let print_profile = sled_table
            .get("print_profile")
            .expect("\x1b[31mErr:\x1b[0m Missing print profile toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse print_profile as bool!");
        let flush_every_ms = sled_table
            .get("flush_every_ms")
            .expect("\x1b[31mErr:\x1b[0m Missing flush_every_ms!")
            .as_integer()
            .expect("\x1b[31mErr:\x1b[0m Could not parse flush_every_ms as int!");

        // Parse sled mode
        let sled_mode_str = sled_table
            .get("mode")
            .expect("\x1b[31mErr:\x1b[0m Missing sled_mode!")
            .as_str()
            .expect("\x1b[31mErr:\x1b[0m Could not parse sled_mode as str!");
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
            if table_name != "blutgang" && table_name != "sled" && table_name != "admin" {
                let rpc_table = parsed_toml.get(table_name).unwrap().as_table().unwrap();

                let max_consecutive = rpc_table
                    .get("max_consecutive")
                    .expect("\x1b[31mErr:\x1b[0m Missing max_consecutive from an RPC!")
                    .as_integer()
                    .expect("\x1b[31mErr:\x1b[0m Could not parse max_consecutive as int!")
                    as u32;
                let url = rpc_table
                    .get("url")
                    .expect("\x1b[31mErr:\x1b[0m Missing URL from RPC!")
                    .as_str()
                    .expect("\x1b[31mErr:\x1b[0m Could not parse URL from a RPC as str!")
                    .to_string();

                let rpc = Rpc::new(url, max_consecutive, ma_length);
                rpc_list.push(rpc);
            }
        }

        // Admin namespace things
        let admin_table = parsed_toml
            .get("admin")
            .expect("\x1b[31mErr:\x1b[0m Missing admin table!")
            .as_table()
            .expect("\x1b[31mErr:\x1b[0m Could not parse admin table!");
        let enabled = admin_table
            .get("enabled")
            .expect("\x1b[31mErr:\x1b[0m Missing admin enabled toggle!")
            .as_bool()
            .expect("\x1b[31mErr:\x1b[0m Could not parse admin enabled as bool!");
        let admin = if enabled {
            let address = admin_table
                .get("address")
                .expect("\x1b[31mErr:\x1b[0m Missing address!")
                .as_str()
                .expect("\x1b[31mErr:\x1b[0m Could not parse admin address!");
            let address = address.replace("localhost", "127.0.0.1");
            let readonly = admin_table
                .get("readonly")
                .expect("\x1b[31mErr:\x1b[0m Missing readonly toggle!")
                .as_bool()
                .expect("\x1b[31mErr:\x1b[0m Could not parse readonly as bool!");
            let jwt = admin_table
                .get("jwt")
                .expect("\x1b[31mErr:\x1b[0m Missing JWT token toggle!")
                .as_bool()
                .expect("\x1b[31mErr:\x1b[0m Could not parse JWT as bool!");

            let key = if jwt {
                admin_table
                    .get("key")
                    .expect("\x1b[31mErr:\x1b[0m Missing key key!")
                    .as_str()
                    .expect("\x1b[31mErr:\x1b[0m Could not parse key as str!")
                    .to_string()
            } else {
                String::new()
            };

            AdminSettings {
                enabled,
                address: address.parse::<SocketAddr>().unwrap(),
                readonly,
                jwt,
                key: DecodingKey::from_secret(key.as_bytes()),
            }
        } else {
            AdminSettings {
                enabled: false,
                address: "127.0.0.1:3001".parse::<SocketAddr>().unwrap(),
                readonly: false,
                jwt: false,
                key: DecodingKey::from_secret(b""),
            }
        };

        if sort_on_startup {
            println!("Sorting RPCs by latency...");
            rpc_list = sort_by_latency(rpc_list, ma_length).await;
        }

        Settings {
            rpc_list,
            do_clear,
            address,
            health_check,
            ttl,
            health_check_ttl,
            sled_config,
            admin,
        }
    }

    fn create_from_matches(matches: ArgMatches) -> Settings {
        // Build the rpc_list
        let rpc_list: String = matches
            .get_one::<String>("rpc_list")
            .expect("Invalid rpc_list")
            .to_string();

        let ma_length = matches
            .get_one::<String>("ma_length")
            .expect("Invalid ma_length");
        let ma_length = ma_length.parse::<f64>().expect("Invalid ma_length");

        // Turn the rpc_list into a csv vec
        let rpc_list: Vec<&str> = rpc_list.split(',').collect();
        let rpc_list: Vec<String> = rpc_list.iter().map(|rpc| rpc.to_string()).collect();
        // Make a list of Rpc structs
        let rpc_list: Vec<Rpc> = rpc_list
            .iter()
            .map(|rpc| Rpc::new(rpc.to_string(), 6, ma_length))
            .collect();

        // Build the SocketAddr
        let address = matches
            .get_one::<String>("address")
            .expect("Invalid address");
        let port = matches.get_one::<String>("port").expect("Invalid port");
        // If the address contains `:` dont concatanate the port and just pass the address
        let address = if address.contains(':') {
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

        let health_check = matches.get_occurrences::<String>("health_check").is_some();
        let ttl = matches
            .get_one::<String>("ttl")
            .expect("Invalid ttl")
            .parse::<u128>()
            .expect("Invalid ttl");
        let health_check_ttl = matches
            .get_one::<String>("health_check_ttl")
            .expect("Invalid health_check_ttl")
            .parse::<u64>()
            .expect("Invalid health_check_ttl");

        // Admin thing setup
        let enabled = matches.get_occurrences::<String>("admin").is_some();
        let admin = if enabled {
            let address = matches
                .get_one::<String>("admin_address")
                .expect("Invalid admin_address");
            let readonly = matches.get_occurrences::<String>("readonly").is_some();
            let jwt = matches.get_occurrences::<String>("jwt").is_some();
            let key = matches.get_one::<String>("key").expect("Invalid key");

            AdminSettings {
                enabled,
                address: address.parse::<SocketAddr>().unwrap(),
                readonly,
                jwt,
                key: DecodingKey::from_secret(key.as_bytes()),
            }
        } else {
            AdminSettings {
                enabled: false,
                address: "::1:3001".parse::<SocketAddr>().unwrap(),
                readonly: false,
                jwt: false,
                key: DecodingKey::from_secret(b""),
            }
        };

        Settings {
            rpc_list,
            do_clear: clear,
            address,
            health_check,
            ttl,
            health_check_ttl,
            sled_config,
            admin,
        }
    }
}
