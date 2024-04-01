//! # Blutgang - the wd40 of ethereum load balancers
//! ![blutgang_gm](https://github.com/rainshowerLabs/blutgang/assets/55022497/ec668c7a-5f56-4b26-8386-f112c2f176ce)
//! 
//! Join the discussion on our [discord](https://discord.gg/92TfQWdjEh), [telegram](https://t.me/rainshower), or [matrix!](https://matrix.to/#/%23rainshower:matrix.org)
//! 
//! Blutgang is a blazing fast, caching, minimalistic load balancer designed with Ethereum's JSON-RPC in mind. Historical RPC queries are cached in a local database, bypassing the need for slow, repeating calls to your node.
//! 
//! For more info about blutgang and how to use it, please check out the [wiki](https://github.com/rainshowerLabs/blutgang/wiki).
//! 
//! ## How to run 
//! 
//! For detailed instructions on how to use blutgang, please read the [wiki](https://github.com/rainshowerLabs/blutgang/wiki).
//! 
//! ### Using cargo
//! 
//! To install blutgang via cargo, run the following command:
//! 
//! ```bash
//! cargo install blutgang
//! ```
//! Once done, grab the `example_config.toml` from this repository, modify it to your liking, and start blutgang with it.
//! 
//! ### From source
//! 
//! Clone the repository, and find the `example_config.toml` file. Edit it to your liking, and run `cargo run --release -- -c example_config.toml`.   
//! 
//! If you want to use command line arguments instead, please run `cargo run --release -- --help` for more info. Keep in mind that the recommended way to run blutgang is via a config file.
//! 
//! ### Max performance
//! 
//! If you need the absolute maximum performance from blutgang, compile it using the command below:
//! 
//! ```bash
//! RUSTFLAGS='-C target-cpu=native' cargo build --profile maxperf
//! ```
//! 
//! ### Docker
//! 
//! The official docker image is available on [dockerhub](https://hub.docker.com/r/makemake1337/blutgang).  
//! You must provide a config file to the docker container, as well as expose the port specified. Example:   
//! ```bash
//! docker run -v /full/path/to/config.toml:/app/config.toml --network host makemake1337/blutgang
//! ```
//! 
//! ### Nix
//! 
//! Using Flakes and the [nix-community/ethereum.nix](https://github.com/nix-community/ethereum.nix) overlay:
//! 
//! ```bash
//! nix run github:nix-community/ethereum.nix#blutgang -- --help
//! ```
//! 
//! ## Benchmarks
//! *Benchmarks were performed with a Ryzen 7 2700X, NVME SSD, and default Ubuntu 23.04 kernel. Same RPC endpoints were used*
//! 
//! ```bash
//! time sothis --source_rpc http://localhost:3000 --mode call_track --contract_address 0x1c479675ad559DC151F6Ec7ed3FbF8ceE79582B6 --origin_block 17885300 --terminal_block 17892269 --calldata 0x06f13056 --query_interval 20
//! ```
//! ![Figure_1](https://github.com/rainshowerLabs/blutgang/assets/55022497/8ce9a690-d2eb-4910-9a5d-807c2bdd4649)
//! ![Figure_2](https://github.com/rainshowerLabs/blutgang/assets/55022497/50d78e5f-2209-488d-82fc-8018388a82e7)
//! 
//! ## Acknowledgements
//! 
//! - [dshackle](https://github.com/emeraldpay/dshackle)
//! - [proxyd](https://github.com/ethereum-optimism/optimism/tree/develop/proxyd)
//! - [web3-proxy](https://github.com/llamanodes/web3-proxy)
//! 
//! Blutgang is standing on the shoulders of giants. Thank you to all the contributors of the projects above!
//! 
//! ## License
//! 
//! Blutgang is licensed under the FSL. Without permission, this means you are allowed to use Blutgang for everything except commercial use cases, such as selling RPC access, and building other load balancers. Internal evaluation and any non-commercial use case is explicitly allowed. Two years after release, that specific version becomes licensed under the Apache-2.0 license.
//! 
//! For commercial support, which includes deployment scripts, relicensing options, ansible playbooks and more, contact us for a commercial license!

mod admin;
mod balancer;
mod config;
mod health;
mod rpc;
mod websocket;

use crate::{
    admin::{
        listener::listen_for_admin_requests,
        liveready::{
            liveness_update_sink,
            LiveReadyUpdate,
            ReadinessState,
        },
    },
    balancer::{
        accept_http::{
            accept_request,
            ConnectionParams,
            RequestChannels,
        },
        processing::CacheArgs,
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

/// `jemalloc` offers faster mallocs when dealing with lots of threads which is what we're doing
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get all the cli args and set them
    let config = Arc::new(RwLock::new(Settings::new(create_match()).await));

    // Copy the configuration values we need
    let (addr, do_clear, do_health_check, admin_enabled, is_ws, expected_block_time) = {
        let config_guard = config.read().unwrap();
        (
            config_guard.address,
            config_guard.do_clear,
            config_guard.health_check,
            config_guard.admin.enabled,
            config_guard.is_ws,
            config_guard.expected_block_time,
        )
    };

    // Make the list a rwlock
    let rpc_list_rwlock = Arc::new(RwLock::new(config.read().unwrap().rpc_list.clone()));

    // Create/Open sled DB
    let cache = config
        .read()
        .unwrap()
        .sled_config
        .open()
        .expect("Can't open/create database!");

    // Cache for storing querries near the tip
    let head_cache = Arc::new(RwLock::new(BTreeMap::<u64, Vec<String>>::new()));

    // Clear database if specified
    if do_clear {
        cache.clear().unwrap();
        log_wrn!("All data cleared from the database.");
    }
    // Insert data about blutgang and our settings into the DB
    //
    // Print any relevant warnings about a misconfigured DB. Check docs for more
    setup_data(cache.clone());

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;
    log_info!("Bound to: {}", addr);

    let (blocknum_tx, blocknum_rx) = watch::channel(0);
    let (finalized_tx, finalized_rx) = watch::channel(0);

    let finalized_rx_arc = Arc::new(finalized_rx.clone());
    let rpc_poverty_list = Arc::new(RwLock::new(config.read().unwrap().poverty_list.clone()));

    // We need liveness status channels even if admin is unused
    let (liveness_tx, liveness_rx) = mpsc::channel(16);

    // Spawn a thread for the admin namespace if enabled
    if admin_enabled {
        let rpc_list_admin = Arc::clone(&rpc_list_rwlock);
        let poverty_list_admin = Arc::clone(&rpc_poverty_list);
        let cache_admin = cache.clone();
        let config_admin = Arc::clone(&config);
        tokio::task::spawn(async move {
            log_info!("Admin namespace enabled, accepting admin methods at admin port");
            let _ = listen_for_admin_requests(
                rpc_list_admin,
                poverty_list_admin,
                cache_admin,
                config_admin,
                liveness_rx,
            )
            .await;
        });
    } else {
        // dont want to deal with potentially dropped channels if admin is disabled?
        // create a sink to immediately drop all messages you receive!
        tokio::task::spawn(liveness_update_sink(liveness_rx));
    }

    // Spawn a thread for the head cache
    let head_cache_clone = Arc::clone(&head_cache);
    let cache_clone = cache.clone();
    let finalized_rxclone = Arc::clone(&finalized_rx_arc);
    tokio::task::spawn(async move {
        let _ = manage_cache(
            &head_cache_clone,
            blocknum_rx,
            finalized_rxclone,
            cache_clone,
        )
        .await;
    });

    // Spawn a thread for the health check
    //
    // Also handle the finalized block tracking in this thread
    let named_blocknumbers = Arc::new(RwLock::new(NamedBlocknumbers::default()));

    if do_health_check {
        let poverty_list_health = Arc::clone(&rpc_poverty_list);
        let config_health = Arc::clone(&config);

        let rpc_list_health = Arc::clone(&rpc_list_rwlock);
        let named_blocknumbers_health = Arc::clone(&named_blocknumbers);
        let liveness_tx_health = liveness_tx.clone();

        tokio::task::spawn(async move {
            let _ = health_check(
                rpc_list_health,
                poverty_list_health,
                finalized_tx,
                liveness_tx_health,
                &named_blocknumbers_health,
                &config_health,
            )
            .await;
        });
    }

    // WebSocket connection + health check setup. Only runs when every node has a WS endpoint.
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<WsconnMessage>();
    let (outgoing_tx, outgoing_rx) = broadcast::channel::<IncomingResponse>(2048);
    let sub_data = Arc::new(SubscriptionData::new());
    if is_ws {
        let (ws_error_tx, ws_error_rx) = mpsc::unbounded_channel::<WsChannelErr>();

        let rpc_list_ws = Arc::clone(&rpc_list_rwlock);
        // TODO: make this more ergonomic
        let ws_handle = Arc::new(RwLock::new(Vec::<
            Option<mpsc::UnboundedSender<serde_json::Value>>,
        >::new()));
        let outgoing_rx_ws = outgoing_rx.resubscribe();
        let incoming_tx_ws = incoming_tx.clone();
        let ws_error_tx_ws = ws_error_tx.clone();

        let sub_dispatcher = Arc::clone(&sub_data);

        tokio::task::spawn(async move {
            tokio::task::spawn(async move {
                let _ =
                    subscription_dispatcher(outgoing_rx_ws, incoming_tx_ws, sub_dispatcher).await;
            });

            let _ = ws_conn_manager(
                rpc_list_ws,
                ws_handle,
                incoming_rx,
                outgoing_tx,
                ws_error_tx_ws,
            )
            .await;
        });

        if do_health_check {
            let dropped_rpc = Arc::clone(&rpc_list_rwlock);
            let dropped_povrty = Arc::clone(&rpc_poverty_list);
            let dropped_inc = incoming_tx.clone();
            let dropped_rx = outgoing_rx.resubscribe();
            let dropped_sub_data = Arc::clone(&sub_data);

            tokio::task::spawn(async move {
                dropped_listener(
                    dropped_rpc,
                    dropped_povrty,
                    ws_error_rx,
                    dropped_inc,
                    dropped_rx,
                    dropped_sub_data,
                )
                .await
            });

            let heads_inc = incoming_tx.clone();
            let heads_rx = outgoing_rx.resubscribe();
            let heads_sub_data = sub_data.clone();

            let cache_args = CacheArgs {
                finalized_rx: finalized_rx.clone(),
                named_numbers: named_blocknumbers.clone(),
                cache: cache.clone(),
                head_cache: head_cache.clone(),
            };

            tokio::task::spawn(async move {
                subscribe_to_new_heads(
                    heads_inc,
                    heads_rx,
                    blocknum_tx,
                    heads_sub_data,
                    cache_args,
                    expected_block_time,
                )
                .await;
            });
        }
    }

    // Send an update to change the state to ready
    let _ = liveness_tx
        .send(LiveReadyUpdate::Readiness(ReadinessState::Ready))
        .await;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, socketaddr) = listener.accept().await?;
        log_info!("Connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let channels = RequestChannels::new(
            finalized_rx_arc.clone(),
            incoming_tx.clone(),
            outgoing_rx.resubscribe(),
        );

        let connection_params = ConnectionParams::new(
            &rpc_list_rwlock,
            channels,
            &named_blocknumbers,
            &head_cache,
            &sub_data,
            cache.clone(),
            &config,
        );

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept!(io, connection_params.clone());
        });
    }
}
