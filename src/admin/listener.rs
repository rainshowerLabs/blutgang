use std::{
    net::SocketAddr,
    sync::{
        Arc,
        RwLock,
    },
};

use sled::Db;

use crate::{
    admin::{
        accept::accept_admin_request,
        liveready::{
            liveness_monitor,
            LiveReadyRequestSnd,
            LiveReadyUpdateRecv,
        },
    },
    log_info,
    Rpc,
    Settings,
};

use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util_blutgang::rt::TokioIo;
use tokio::{
    net::TcpListener,
    sync::mpsc,
};

macro_rules! accept_admin {
    (
        $io:expr,
        $rpc_list_rwlock:expr,
        $poverty_list_rwlock:expr,
        $cache:expr,
        $config:expr,
        $liveness_request_tx:expr,
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_admin_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        Arc::clone($poverty_list_rwlock),
                        $cache.clone(),
                        Arc::clone($config),
                        $liveness_request_tx.clone(),
                    );
                    response
                }),
            )
            .await
        {
            println!("error serving admin connection: {:?}", err);
        }
    };
}

async fn admin_api_server(
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    poverty_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: Db,
    config: Arc<RwLock<Settings>>,
    address: SocketAddr,
    liveness_request_tx: LiveReadyRequestSnd,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a listener and bind to it
    let listener = TcpListener::bind(address).await?;
    log_info!("Bound admin API to: {}", address);

    loop {
        let (stream, socketaddr) = listener.accept().await?;
        log_info!("Admin connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let rpc_list_rwlock_clone = Arc::clone(&rpc_list_rwlock);
        let poverty_list_rwlock_clone = Arc::clone(&poverty_list_rwlock);
        let cache_clone = cache.clone();
        let config_clone = Arc::clone(&config);
        let liveness_request_tx_clone = liveness_request_tx.clone();

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept_admin!(
                io,
                &rpc_list_rwlock_clone,
                &poverty_list_rwlock_clone,
                &cache_clone,
                &config_clone,
                &liveness_request_tx_clone,
            );
        });
    }
}

/// Used for listening to admin requests as its own tokio task.
/// Also used for k8s liveness/readiness probes.
///
/// Similar to what you'd find in main/balancer
pub async fn listen_for_admin_requests(
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    poverty_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: Db,
    config: Arc<RwLock<Settings>>,
    liveness_receiver: LiveReadyUpdateRecv,
) -> Result<(), Box<dyn std::error::Error>> {
    let address;
    {
        let config_guard = config.read().unwrap();
        address = config_guard.admin.address;
    }

    // Spawn thread for monitoring the current liveness status of Blutgang
    let (liveness_request_tx, liveness_request_rx) = mpsc::channel(16);
    tokio::spawn(liveness_monitor(liveness_receiver, liveness_request_rx));

    admin_api_server(
        rpc_list_rwlock,
        poverty_list_rwlock,
        cache,
        config,
        address,
        liveness_request_tx,
    )
    .await
}
