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
    },
    rpc::types::Rpc,
};

use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::{
        Arc,
        RwLock,
    },
};

use sled::IVec;
use tokio::net::TcpListener;
use tokio::sync::watch;

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
    // Cache for storing querries near the tip
    let head_cache = Arc::new(RwLock::new(BTreeMap::<u64, HashMap<String, IVec>>::new()));

    // Clear database if specified
    if config.do_clear {
        cache.clear().unwrap();
        println!("All data cleared from the database.");
    }

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(config.address).await?;
    println!("Bound to: {}", config.address);

    // Spawn a thread for the health check
    //
    // Also handle the finalized block tracking in this thread
    let rpc_list_health = Arc::clone(&rpc_list_rwlock);
    let rpc_poverty_list = Arc::new(RwLock::new(Vec::<Rpc>::new()));
    let (blocknum_tx, blocknum_rx) = watch::channel(0);

    if config.health_check {
        tokio::task::spawn(async move {
            let _ = health_check(
                rpc_list_health,
                rpc_poverty_list,
                blocknum_tx,
                config.ttl,
                config.health_check_ttl,
            )
            .await;
        });
    }

    // Spawn a thread for the head cache
    let head_cache_clone = Arc::clone(&head_cache);
    let cache_clone = Arc::clone(&cache);
    let blocknum_rx_clone = blocknum_rx.clone();
    tokio::task::spawn(async move {
        let _ = manage_cache(&head_cache_clone, blocknum_rx_clone.clone(), &cache_clone).await;
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

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(
                io,
                &rpc_list_rwlock_clone,
                head_cache_clone,
                blocknum_rx.clone(),
                &cache_clone,
                config.ttl
            );
        });
    }
}
