mod admin;
mod balancer;
mod config;
mod health;
mod rpc;
mod websocket;

use dashmap::DashMap;
use serde_json::Value;

use crate::{
    admin::listener::listen_for_admin_requests,
    balancer::accept_http::{
        accept_request,
        RequestChannels,
    },
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
    websocket::{
        client::ws_conn_manager,
        subscription_manager::{
            RequestResult,
            subscription_dispatcher,
        },
    },
};

use std::{
    collections::BTreeMap,
    println,
    sync::{
        Arc,
        RwLock,
    },
};

use tokio::{
    net::TcpListener,
    sync::{
        broadcast,
        mpsc,
        watch,
    },
};

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

    // Copy the configuration values we need
    let (addr_clone, do_clear_clone, health_check_clone, admin_enabled_clone) = {
        let config_guard = config.read().unwrap();
        (
            config_guard.address,
            config_guard.do_clear,
            config_guard.health_check,
            config_guard.admin.enabled,
        )
    };

    // Make the list a rwlock
    let rpc_list_rwlock = Arc::new(RwLock::new(config.read().unwrap().rpc_list.clone()));

    // Create/Open sled DB
    let cache = Arc::new(config.read().unwrap().sled_config.open().unwrap());

    // Cache for storing querries near the tip
    let head_cache = Arc::new(RwLock::new(BTreeMap::<u64, Vec<String>>::new()));

    // Clear database if specified
    if do_clear_clone {
        // Also drop the "subscriptions" tree
        let _ = cache.drop_tree("subscriptions");
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

    // websocket connections
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<Value>();
    let (outgoing_tx, outgoing_rx) = broadcast::channel::<Value>(256);

    // Map of user ids to channels
    let sink_map = Arc::new(DashMap::<u64, mpsc::UnboundedSender<RequestResult>>::new());

    // Map of subscriptions to users
    // TODO: I feel like this is sub optimal for performance
    let subscribed_users = Arc::new(DashMap::<u64, DashMap::<u64, bool>>::new());

    let rpc_list_ws = Arc::clone(&rpc_list_rwlock);
    let sink_map_ws = Arc::clone(&sink_map);
    let subscribed_users_ws = Arc::clone(&subscribed_users);
    let outgoing_rx_ws = outgoing_rx.resubscribe();

    tokio::task::spawn(async move {
        subscription_dispatcher(outgoing_rx_ws, sink_map_ws, subscribed_users_ws);
        let _ = ws_conn_manager(rpc_list_ws, incoming_rx, outgoing_tx).await;
    });

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
        let sink_map_clone = Arc::clone(&sink_map);
        let subscribed_users_clone = Arc::clone(&subscribed_users);

        let channels = RequestChannels {
            finalized_rx: finalized_rx_clone,
            incoming_tx: incoming_tx.clone(),
            outgoing_rx: outgoing_rx.resubscribe(),
        };

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(
                io,
                &rpc_list_rwlock_clone,
                &cache_clone,
                channels.clone(),
                &named_blocknumbers_clone,
                &head_cache_clone,
                &sink_map_clone,
                &subscribed_users_clone,
                &config_clone
            );
        });
    }
}
