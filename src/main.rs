mod balancer;
mod rpc;

use crate::{
    balancer::balancer::*,
    rpc::types::Rpc,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::{
    net::TcpListener,
};

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

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Bound to: {}", addr);

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

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Get the next Rpc in line
            let rpc;
            {
                let mut last = last_mtx_clone.lock().unwrap();
                let rpc_list = rpc_list_mtx_clone.lock().unwrap();

                println!("last: {:?}", last);
                let now;
                (rpc, now) = pick(&rpc_list, *last);
                *last = now;
            }

            // Finally, we bind the incoming connection to our service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(move |req| forward(req, rpc)))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
