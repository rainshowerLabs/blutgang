mod balancer;
mod rpc;

use crate::{
    balancer::balancer::forward,
    rpc::types::Rpc,
};
use std::net::SocketAddr;
use std::sync::{
    Arc,
    Mutex,
};

use hyper::{
    server::conn::http1,
    service::service_fn,
};

use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use clap::{
    Arg,
    Command,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("blutgang")
        .version("0.1.0")
        .author("makemake <vukasin@gostovic.me>")
        .about("Tool for replaying historical transactions. Designed to be used with anvil or hardhat.")
        .arg(Arg::new("rpc_list")
            .long("rpc_list")
            .short('r')
            .num_args(1..)
            .default_value("")
            .required(true)
            .help("CSV list of rpcs"))
        .arg(Arg::new("port")
            .long("port")
            .short('p')
            .num_args(1..)
            .default_value("3000")
            .help("port to listen to"))
        .arg(Arg::new("db")
            .long("db")
            .short('d')
            .num_args(1..)
            .default_value("blutgang-cache")
            .help("Database path"))
        .arg(Arg::new("clear")
            .long("clear")
            .num_args(0..)
            .help("Clear cache"))
    .get_matches();

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
    // Make the list a mutex
    let rpc_list_mtx = Arc::new(Mutex::new(rpc_list));

    let port = matches.get_one::<String>("port").expect("Invalid port");

    let addr = SocketAddr::from(([127, 0, 0, 1], port.parse::<u16>().unwrap()));
    
    println!("Bound to: {}", addr);

    // Create/Configure/Open sled DB
    let db_path = matches.get_one::<String>("db").expect("Invalid db path");

    let _config = sled::Config::default()
        .path(db_path)
        .mode(sled::Mode::HighThroughput)
        .cache_capacity(1_000_000_000)
        .print_profile_on_drop(true)
        .flush_every_ms(Some(1000));
    let cache: Arc<sled::Db> = Arc::new(_config.open().unwrap());

    // Check if the clear flag is set
    let clear = matches.get_occurrences::<String>("clear").is_some();

    if clear {
        cache.clear().unwrap();
        println!("All data cleared from the database.");
    }

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // Create a counter to keep track of the last rpc, max so it overflows
    let last_mtx: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Clone the shared `rpc_list_mtx` and `last_mtx` for use in the closure
        let rpc_list_mtx_clone = Arc::clone(&rpc_list_mtx);
        let last_mtx_clone = Arc::clone(&last_mtx);
        let cache_clone = Arc::clone(&cache);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        forward(
                            req,
                            Arc::clone(&rpc_list_mtx_clone),
                            Arc::clone(&last_mtx_clone),
                            Arc::clone(&cache_clone),
                        )
                    }),
                )
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
