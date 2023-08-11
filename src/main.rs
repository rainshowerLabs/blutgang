mod balancer;
mod config;
mod rpc;

use crate::{
    balancer::balancer::forward,
    config::cli_args::create_match,
    config::setup::set_args,
    rpc::types::Rpc,
};

use std::net::SocketAddr;
use std::sync::{
    Arc,
    Mutex,
};
use tokio::net::TcpListener;

use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get all the cli args amd set them
    let config = set_args(create_match());

    // Make the list a mutex
    let rpc_list_mtx = Arc::new(Mutex::new(config.rpc_list));

    let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
    println!("Bound to: {}", addr);

    // Create/Configure/Open sled DB
    let sled_config = sled::Config::default()
        .path(config.db_path)
        .mode(sled::Mode::HighThroughput)
        .cache_capacity(config.cache_capacity)
        .print_profile_on_drop(config.print_profile)
        .flush_every_ms(config.flush_time);
    let cache: Arc<sled::Db> = Arc::new(sled_config.open().unwrap());

    // Clear database if specified
    if config.do_clear {
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
