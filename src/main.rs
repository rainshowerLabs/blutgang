mod balancer;
mod config;
mod health;
mod rpc;

use crate::{
    balancer::balancer::accept_request,
    config::{
        cli_args::create_match,
        types::Settings,
    },
    health::{
        check::health_check,
        head_cache::manage_cache,
        safe_block::NamedBlocknumbers,
    },
    rpc::types::Rpc,
};

use std::{
    collections::BTreeMap,
    println,
    sync::{
        Arc,
        RwLock,
    },
};

use tokio::net::TcpListener;
use tokio::sync::watch;

use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util_blutgang::rt::TokioIo;

// jeemalloc offers faster mallocs when dealing with lots of threads which is what we're doing
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
        [
            176, 76, 1, 109, 13, 127, 134, 25, 55, 111, 28, 182, 82, 155, 135, 143, 204, 161, 53,
            4, 158, 140, 22, 219, 138, 5, 57, 150, 8, 154, 17, 252,
        ],
        "{\"jsonrpc\":\"2.0\",\"id\":null,\"result\":\"blutgang v0.2.0 nc \
        Dedicated to the spirit that lives inside of the computer\"}",
    );

    // Cache for storing querries near the tip
    let head_cache = Arc::new(RwLock::new(BTreeMap::<u64, Vec<String>>::new()));

    // Clear database if specified
    if config.do_clear {
        cache.clear().unwrap();
        println!("\x1b[93mWrn:\x1b[0m All data cleared from the database.");
    }

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(config.address).await?;
    println!("\x1b[35mInfo:\x1b[0m Bound to: {}", config.address);

    // Spawn a thread for the health check
    //
    // Also handle the finalized block tracking in this thread
    let rpc_list_health = Arc::clone(&rpc_list_rwlock);
    let rpc_poverty_list = Arc::new(RwLock::new(Vec::<Rpc>::new()));
    let named_blocknumbers = Arc::new(RwLock::new(NamedBlocknumbers::default()));
    let named_blocknumbers_health = Arc::clone(&named_blocknumbers);
    let (blocknum_tx, blocknum_rx) = watch::channel(0);
    let (finalized_tx, finalized_rx) = watch::channel(0);

    if config.health_check {
        tokio::task::spawn(async move {
            let _ = health_check(
                rpc_list_health,
                rpc_poverty_list,
                &blocknum_tx,
                finalized_tx,
                &named_blocknumbers_health,
                config.ttl,
                config.health_check_ttl,
            )
            .await;
        });
    }

    // Spawn a thread for the head cache
    let head_cache_clone = Arc::clone(&head_cache);
    let cache_clone = Arc::clone(&cache);
    let finalized_rxclone = finalized_rx.clone();
    tokio::task::spawn(async move {
        let _ = manage_cache(
            &head_cache_clone,
            blocknum_rx,
            finalized_rxclone,
            &cache_clone,
        )
        .await;
    });

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Clone the shared `rpc_list_rwlock` and cache for use in the closure
        let rpc_list_rwlock_clone = Arc::clone(&rpc_list_rwlock);
        let cache_clone = Arc::clone(&cache);
        let head_cache_clone = Arc::clone(&head_cache);
        let finalized_rx_clone = finalized_rx.clone();
        let named_blocknumbers_clone = Arc::clone(&named_blocknumbers);
        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(
                io,
                &rpc_list_rwlock_clone,
                config.ma_length,
                &cache_clone,
                &finalized_rx_clone,
                &named_blocknumbers_clone,
                &head_cache_clone,
                config.ttl
            );
        });
    }
}
