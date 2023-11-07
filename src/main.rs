mod admin;
mod balancer;
mod config;
mod health;
mod rpc;

use crate::{
    admin::listener::listen_for_admin_requests,
    balancer::balancer::accept_request,
    config::{
        cache_setup::setup_data,
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
    let config = Arc::new(RwLock::new(Settings::new(create_match()).await));
    let binding = Arc::clone(&config);
    let config_guard = binding.read().unwrap();

    // Clone all the things we need so we dont send across await
    let addr_clone = config_guard.address;
    let do_clear_clone = config_guard.do_clear;
    let health_check_clone = config_guard.health_check;
    let admin_enabled_clone = config_guard.admin.enabled;

    // Make the list a rwlock
    let rpc_list_rwlock = Arc::new(RwLock::new(config_guard.rpc_list.clone()));

    // Create/Open sled DB
    let cache: Arc<sled::Db> = Arc::new(config_guard.sled_config.open().unwrap());

    // Drop the config guard used to get the settings
    // Prevents deadlocks when writing
    drop(config_guard);
    drop(binding);

    // Cache for storing querries near the tip
    let head_cache = Arc::new(RwLock::new(BTreeMap::<u64, Vec<String>>::new()));

    // Clear database if specified
    if do_clear_clone {
        cache.clear().unwrap();
        println!("\x1b[93mWrn:\x1b[0m All data cleared from the database.");
    }
    // Insert data about blutgang and our settings into the DB
    //
    // Print any relevant warnings about a misconfigured DB. Check docs for more
    setup_data(Arc::clone(&cache));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr_clone).await?;
    println!("\x1b[35mInfo:\x1b[0m Bound to: {}", addr_clone);

    // Spawn a thread for the health check
    //
    // Also handle the finalized block tracking in this thread
    let rpc_poverty_list = Arc::new(RwLock::new(Vec::<Rpc>::new()));
    let named_blocknumbers = Arc::new(RwLock::new(NamedBlocknumbers::default()));
    let (blocknum_tx, blocknum_rx) = watch::channel(0);
    let (finalized_tx, finalized_rx) = watch::channel(0);
    let finalized_rx_arc = Arc::new(finalized_rx);

    if health_check_clone {
        let rpc_list_health = Arc::clone(&rpc_list_rwlock);
        let poverty_list_health = Arc::clone(&rpc_poverty_list);
        let named_blocknumbers_health = Arc::clone(&named_blocknumbers);
        let config_health = Arc::clone(&config);
        
        tokio::task::spawn(async move {
            let _ = health_check(
                rpc_list_health,
                poverty_list_health,
                &blocknum_tx,
                finalized_tx,
                &named_blocknumbers_health,
                &config_health,
            )
            .await;
        });
    }

    // Spawn a thread for the admin namespace if enabled
    if admin_enabled_clone {
        let rpc_list_admin = Arc::clone(&rpc_list_rwlock);
        let poverty_list_admin = Arc::clone(&rpc_poverty_list);
        let cache_admin = Arc::clone(&cache);
        let config_admin = Arc::clone(&config);
        tokio::task::spawn(async move {
            println!("\x1b[35mInfo:\x1b[0m Admin namespace enabled, accepting admin methods at admin port");
            let _ = listen_for_admin_requests(
                rpc_list_admin,
                poverty_list_admin,
                cache_admin,
                config_admin,
            )
            .await;
        });
    }

    // Spawn a thread for the head cache
    let head_cache_clone = Arc::clone(&head_cache);
    let cache_clone = Arc::clone(&cache);
    let finalized_rxclone = Arc::clone(&finalized_rx_arc);
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
        let (stream, socketaddr) = listener.accept().await?;
        println!("\x1b[35mInfo:\x1b[0m Connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Clone the shared `rpc_list_rwlock` and cache for use in the closure
        let rpc_list_rwlock_clone = Arc::clone(&rpc_list_rwlock);
        let cache_clone = Arc::clone(&cache);
        let head_cache_clone = Arc::clone(&head_cache);
        let finalized_rx_clone = Arc::clone(&finalized_rx_arc);
        let named_blocknumbers_clone = Arc::clone(&named_blocknumbers);
        let config_clone = Arc::clone(&config);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(
                io,
                &rpc_list_rwlock_clone,
                &cache_clone,
                &finalized_rx_clone,
                &named_blocknumbers_clone,
                &head_cache_clone,
                &config_clone
            );
        });
    }
}
