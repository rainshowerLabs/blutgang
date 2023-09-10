mod balancer;
mod config;
mod health;
mod rpc;
#[cfg(feature = "tui")]
mod tui;

use crate::{
    balancer::balancer::accept_request,
    config::{
        cli_args::create_match,
        types::Settings,
    },
    health::check::health_check,
    rpc::types::Rpc,
};

use std::sync::{
    Arc,
    Mutex,
    RwLock,
};
use tokio::net::TcpListener;

use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::TokioIo;

// jeemallocator *should* offer faster mallocs when dealing with lots of threads which is what we're doing
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get all the cli args amd set them
    let config = Settings::new(create_match()).await;

    // Make the list a rwlock
    let rpc_list_rwlock = Arc::new(RwLock::new(config.rpc_list.clone()));

    // Create/Open sled DB
    let cache: Arc<sled::Db> = Arc::new(config.sled_config.open().unwrap());
    // Insert kv pair `blutgang_is_lb` `true` to know what we're interacting with
    // `blutgang_is_lb` is cached as a blake3 cache
    let _ = cache.insert(
        "172cf910abc64d0fe6b243766b3ae9f56f32978d7c144ddadde9c615ea38891d",
        "true",
    );

    // Clear database if specified
    if config.do_clear {
        cache.clear().unwrap();
        println!("All data cleared from the database.");
    }

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(config.address).await?;
    println!("Bound to: {}", config.address);

    // Create a counter to keep track of the last rpc, max so it overflows
    let last_mtx: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));

    // Create channel for response passing and spawn tui if the feature is enabled
    #[cfg(feature = "tui")]
    let response_list = Arc::new(RwLock::new(Vec::new()));
    #[cfg(feature = "tui")]
    {
        use tui::terminal::*;
        // We're passing the rpc list as an arc to the ui thread.
        // TODO: This is blocking writes. Make it potentially unsafe or add message passing???
        let rpc_list_tui = Arc::clone(&rpc_list_rwlock);

        let config_clone = config.clone();
        let response_list_clone = Arc::clone(&response_list);

        tokio::task::spawn(async move {
            let mut terminal = setup_terminal().unwrap();
            let _ = run_tui(
                &mut terminal,
                config_clone,
                &rpc_list_tui,
                &response_list_clone,
            )
            .await;
        });
    }

    // Spawn a thread for the health check
    let rpc_list_health = Arc::clone(&rpc_list_rwlock);
    let rpc_poverty_list = Arc::new(RwLock::new(Vec::<Rpc>::new()));

    tokio::task::spawn(async move {
        let _ = health_check(
            rpc_list_health,
            rpc_poverty_list,
            config.ttl,
            config.health_check_ttl,
        )
        .await;
    });

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Clone the shared `rpc_list_rwlock` and `last_mtx` for use in the closure
        let rpc_list_rwlock_clone = Arc::clone(&rpc_list_rwlock);
        let last_mtx_clone = Arc::clone(&last_mtx);
        let cache_clone = Arc::clone(&cache);
        #[cfg(feature = "tui")]
        let response_list_clone = Arc::clone(&response_list);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            #[cfg(not(feature = "tui"))]
            accept!(
                io,
                &rpc_list_rwlock_clone,
                &last_mtx_clone,
                config.ma_length,
                &cache_clone
            );
            #[cfg(feature = "tui")]
            accept!(
                io,
                &rpc_list_rwlock_clone,
                &last_mtx_clone,
                config.ma_length,
                &cache_clone,
                &response_list_clone
            );
        });
    }
}
