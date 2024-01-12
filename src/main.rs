mod admin;
mod balancer;
mod config;
mod health;
mod rpc;
mod websocket;

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
        check::{
            dropped_listener,
            health_check,
        },
        head_cache::manage_cache,
        safe_block::{
            subscribe_to_new_heads,
            NamedBlocknumbers,
        },
    },
    rpc::types::Rpc,
    websocket::{
        client::ws_conn_manager,
        subscription_manager::subscription_dispatcher,
        types::{
            IncomingResponse,
            SubscriptionData,
            WsChannelErr,
            WsconnMessage,
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

    // websocket connections
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<WsconnMessage>();
    let (outgoing_tx, outgoing_rx) = broadcast::channel::<IncomingResponse>(256);
    let (ws_error_tx, ws_error_rx) = mpsc::unbounded_channel::<WsChannelErr>();

    let rpc_list_ws = Arc::clone(&rpc_list_rwlock);
    // TODO: make this more ergonomic
    let ws_handle = Arc::new(RwLock::new(Vec::<
        Option<mpsc::UnboundedSender<serde_json::Value>>,
    >::new()));
    let outgoing_rx_ws = outgoing_rx.resubscribe();
    let ws_error_tx_ws = ws_error_tx.clone();

    let sub_data = Arc::new(SubscriptionData::new());
    let sub_dispatcher = Arc::clone(&sub_data);

    tokio::task::spawn(async move {
        subscription_dispatcher(outgoing_rx_ws, sub_dispatcher);
        let _ = ws_conn_manager(
            rpc_list_ws,
            ws_handle,
            incoming_rx,
            outgoing_tx,
            ws_error_tx_ws,
        )
        .await;
    });

    let (blocknum_tx, blocknum_rx) = watch::channel(0);
    let (finalized_tx, finalized_rx) = watch::channel(0);

    let finalized_rx_arc = Arc::new(finalized_rx);
    let rpc_poverty_list = Arc::new(RwLock::new(Vec::<Rpc>::new()));

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

    // Spawn a thread for the health check
    //
    // Also handle the finalized block tracking in this thread
    let named_blocknumbers = Arc::new(RwLock::new(NamedBlocknumbers::default()));

    if health_check_clone {
        let rpc_list_health = Arc::clone(&rpc_list_rwlock);
        let poverty_list_health = Arc::clone(&rpc_poverty_list);
        let named_blocknumbers_health = Arc::clone(&named_blocknumbers);
        let config_health = Arc::clone(&config);
        let health_check_ttl = config.read().unwrap().health_check_ttl;

        let dropped_rpc = rpc_list_health.clone();
        let dropped_povrty = poverty_list_health.clone();
        let dropped_inc = incoming_tx.clone();
        tokio::task::spawn(async move {
            dropped_listener(dropped_rpc, dropped_povrty, ws_error_rx, dropped_inc).await
        });

        tokio::task::spawn(async move {
            subscribe_to_new_heads(
                &rpc_list_health,
                &named_blocknumbers_health,
                ws_error_tx,
                health_check_ttl,
            )
            .await;
        });

        let rpc_list_health = Arc::clone(&rpc_list_rwlock);
        let named_blocknumbers_health = Arc::clone(&named_blocknumbers);

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
        let sub_data_clone = Arc::clone(&sub_data);

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
                sub_data_clone.clone(),
                &config_clone
            );
        });
    }
}
