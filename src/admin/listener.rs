use std::sync::{
    Arc,
    RwLock,
};

use sled::Db;

use crate::{
    Settings,
    Rpc,
    admin::accept::accept_admin_request
};

use tokio::net::TcpListener;
use hyper::{
    server::conn::http1,
    service::service_fn,
};
use hyper_util_blutgang::rt::TokioIo;

macro_rules! accept_admin {
    (
        $io:expr,
        $rpc_list_rwlock:expr,
        $cache:expr,
        $config:expr,
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
                        Arc::clone($cache),
                        Arc::clone($config),
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

pub async fn listen_for_admin_requests(
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> Result<(), Box<dyn std::error::Error>>{
    let address;
    {
        let config_guard = config.read().unwrap();
        address = config_guard.admin.address.clone();
    }

    // Create a listener and bind to it
    let listener = TcpListener::bind(address).await?;

    loop {
        let (stream, socketaddr) = listener.accept().await?;
        println!("\x1b[35mInfo:\x1b[0m Admin connection from: {}", socketaddr);

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let rpc_list_rwlock_clone = Arc::clone(&rpc_list_rwlock);
        let cache_clone = Arc::clone(&cache);
        let config_clone = Arc::clone(&config);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            accept_admin!(
                io,
                &rpc_list_rwlock_clone,
                &cache_clone,
                &config_clone,
            );
        });
    }
}
