use crate::{
    balancer::{
        format::{
            incoming_to_value,
            replace_block_tags,
        },
        processing::{
            cache_query,
            update_rpc_latency,
            CacheArgs,
        },
        selection::select::pick,
    },
    cache_error,
    database::types::GenericBytes,
    db_get,
    no_rpc_available,
    print_cache_error,
    rpc::types::Rpc,
    rpc_response,
    timed_out,
    websocket::{
        server::serve_websocket,
        types::{
            IncomingResponse,
            SubscriptionData,
        },
    },
    Settings,
    WsconnMessage,
};

use tokio::sync::{
    broadcast,
    mpsc,
    watch,
};

use serde_json::Value;

// Select either blake3 or xxhash based on the features
#[cfg(not(feature = "xxhash"))]
use blake3::hash;

#[cfg(feature = "xxhash")]
use xxhash_rust::xxh3::xxh3_64;
#[cfg(feature = "xxhash")]
use zerocopy::AsBytes; // Impls AsBytes trait for u64

use http_body_util::Full;
use hyper::{
    body::Bytes,
    header::HeaderValue,
    Request,
};
use hyper_tungstenite::{
    is_upgrade_request,
    upgrade,
};

use tokio::time::timeout;

use std::{
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
    time::{
        Duration,
        Instant,
    },
};

/// `ConnectionParams` contains the necessary data needed for blutgang
/// to fulfil an incoming request.
#[derive(Clone)]
pub struct ConnectionParams {
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    channels: RequestChannels,
    sub_data: Arc<SubscriptionData>,
    config: Arc<RwLock<Settings>>,
}

impl ConnectionParams {
    pub fn new(
        rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
        channels: RequestChannels,
        sub_data: &Arc<SubscriptionData>,
        config: &Arc<RwLock<Settings>>,
    ) -> Self {
        ConnectionParams {
            rpc_list: rpc_list_rwlock.clone(),
            channels,
            sub_data: sub_data.clone(),
            config: config.clone(),
        }
    }
}

pub struct RequestParams {
    pub ttl: u128,
    pub max_retries: u32,
    pub header_check: bool,
}

#[derive(Debug)]
pub struct RequestChannels {
    pub finalized_rx: Arc<watch::Receiver<u64>>,
    pub incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    pub outgoing_rx: broadcast::Receiver<IncomingResponse>,
}

impl RequestChannels {
    pub fn new(
        finalized_rx: Arc<watch::Receiver<u64>>,
        incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
        outgoing_rx: broadcast::Receiver<IncomingResponse>,
    ) -> Self {
        Self {
            finalized_rx,
            incoming_tx,
            outgoing_rx,
        }
    }
}

impl Clone for RequestChannels {
    fn clone(&self) -> Self {
        Self {
            finalized_rx: Arc::clone(&self.finalized_rx),
            incoming_tx: self.incoming_tx.clone(),
            outgoing_rx: self.outgoing_rx.resubscribe(),
        }
    }
}

/// Macros for accepting requests
#[macro_export]
macro_rules! accept {
    (
        $io:expr,
        $cache_args:expr,
        $connection_params:expr
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response =
                        accept_request(req, $cache_args.clone(), $connection_params.clone());
                    response
                }),
            )
            .with_upgrades()
            .await
        {
            tracing::error!(?err, "Error serving connection");
        }
    };
}

/// Macro for getting responses from either the cache or RPC nodes
macro_rules! get_response {
    (
        $tx:expr,
        $cache_args:expr,
        $tx_hash:expr,
        $rpc_position:expr,
        $id:expr,
        $con_params:expr,
        $ttl:expr,
        $max_retries:expr
    ) => {
        match db_get!($cache_args.cache, $tx_hash.as_bytes().to_owned().into()) {
            Ok(Some(mut rax)) => {
                $rpc_position = None;
                // Reconstruct ID
                let mut cached: Value = simd_json::serde::from_slice(rax.as_mut()).unwrap();

                cached["id"] = $id.into();
                cached.to_string()
            }
            Ok(_) => {
                fetch_from_rpc!(
                    $tx,
                    $cache_args,
                    $tx_hash,
                    $rpc_position,
                    $id,
                    $con_params,
                    $ttl,
                    $max_retries
                )
            }
            Err(_) => {
                // If anything errors send an rpc request and see if it works, if not then gg
                print_cache_error!();
                $rpc_position = None;
                return (cache_error!(), $rpc_position);
            }
        }
    };
}

macro_rules! fetch_from_rpc {
    (
        $tx:expr,
        $cache_args:expr,
        $tx_hash:expr,
        $rpc_position:expr,
        $id:expr,
        $con_params:expr,
        $ttl:expr,
        $max_retries:expr
    ) => {{
        // Kinda jank but set the id back to what it was before
        $tx["id"] = $id.into();

        // Loop until we get a response
        let mut rx;
        let mut retries = 0;
        loop {
            // Get the next Rpc in line.
            let mut rpc;
            {
                let mut rpc_list_guard = $con_params.rpc_list.write().unwrap_or_else(|e| {
                    // Handle the case where the RwLock is poisoned
                    e.into_inner()
                });

                (rpc, $rpc_position) = pick(&mut rpc_list_guard);
            }
            tracing::info!(rpc.name, "Forwarding to");

            // Check if we have any RPCs in the list, if not return error
            if $rpc_position == None {
                return (no_rpc_available!(), None);
            }

            // Send the request. And return a timeout if it takes too long
            //
            // Check if it contains any errors or if its `latest` and insert it if it isn't
            match timeout(
                Duration::from_millis($ttl.try_into().unwrap()),
                rpc.send_request($tx.clone()),
            )
            .await
            {
                Ok(rxa) => {
                    rx = rxa.unwrap();
                    break;
                }
                Err(_) => {
                    tracing::warn!("An RPC request has timed out, picking new RPC and retrying.");
                    rpc.update_latency($ttl as f64);
                    retries += 1;
                }
            };

            if retries == $max_retries {
                return (timed_out!(), $rpc_position);
            }
        }

        // Don't cache responses that contain errors or missing trie nodes
        cache_query(&mut rx, $tx, $tx_hash, &$cache_args);

        rx
    }};
}

/// Pick RPC and send request to it. In case the result is cached,
/// read and return from the cache.
pub async fn forward_body<K, V>(
    tx: Request<hyper::body::Incoming>,
    con_params: &ConnectionParams,
    cache_args: CacheArgs<K, V>,
    params: RequestParams,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
)
where
    K: GenericBytes + From<[u8; 32]>,
    V: GenericBytes + From<Vec<u8>>,
{
    // TODO: do content type validation more upstream
    // Check if body has application/json
    //
    // Can be toggled via the config. Should be on if we want blutgang to be JSON-RPC compliant.
    if params.header_check
        && tx.headers().get("content-type") != Some(&HeaderValue::from_static("application/json"))
    {
        return (
            Ok(hyper::Response::builder()
                .status(400)
                .body(Full::new(Bytes::from("Improper content-type header")))
                .unwrap()),
            None,
        );
    }

    // Convert incoming body to serde value
    let mut tx = incoming_to_value(tx).await.unwrap();

    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // Hash the request with either blake3 or xxhash depending on the enabled feature
    let tx_hash;
    #[cfg(not(feature = "xxhash"))]
    {
        tx_hash = hash(tx.to_string().as_bytes());
    }
    #[cfg(feature = "xxhash")]
    {
        tx_hash = xxh3_64(tx.to_string().as_bytes());
    }

    // RPC used to get the response, we use it to update the latency for it later.
    let mut rpc_position;

    // Rewrite named block parameters if possible
    let mut tx = replace_block_tags(&mut tx, &cache_args.named_numbers);

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache_args,
        tx_hash,
        rpc_position,
        id,
        con_params,
        params.ttl,
        params.max_retries
    );

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    // Build the response
    let res = hyper::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap();

    (Ok(res), rpc_position)
}

/// Forward the request to *a* RPC picked by the algo set by the user.
/// Measures the time needed for a request, and updates the respective
/// RPC lself.
/// In case of a timeout, returns an error.
pub async fn accept_request<K, V>(
    mut tx: Request<hyper::body::Incoming>,
    connection_params: ConnectionParams,
    cache_args: CacheArgs<K, V>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible>
where
    K: GenericBytes + From<[u8; 32]> + 'static,
    V: GenericBytes + From<Vec<u8>> + 'static,
{
    // Check if the request is a websocket upgrade request.
    if is_upgrade_request(&tx) {
        tracing::info!("Received WS upgrade request");

        if !connection_params.config.read().unwrap().is_ws {
            return rpc_response!(
                500,
                Full::new(Bytes::from(
                    "{code:-32005, message:\"error: WebSockets are disabled!\"}".to_string(),
                ))
            );
        }

        let (response, websocket) = match upgrade(&mut tx, None) {
            Ok((response, websocket)) => (response, websocket),
            Err(e) => {
                tracing::error!(?e, "Websocket upgrade error");
                return rpc_response!(500, Full::new(Bytes::from(
                    "{code:-32004, message:\"error: Websocket upgrade error! Try again later...\"}"
                        .to_string(),
                )));
            }
        };

        // Spawn a task to handle the websocket connection.
        tokio::task::spawn(async move {
            if let Err(e) = serve_websocket(
                websocket,
                connection_params.channels.incoming_tx,
                connection_params.channels.outgoing_rx,
                connection_params.sub_data.clone(),
                cache_args.to_owned(),
            )
            .await
            {
                tracing::error!(?e, "Websocket connection error");
            }
        });

        // Return the response so the spawned future can continue.
        return Ok(response);
    }

    // Send request
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    let rpc_position: Option<usize>;

    // RequestParams from config
    let params = {
        let config_guard = connection_params.config.read().unwrap();
        RequestParams {
            ttl: config_guard.ttl,
            max_retries: config_guard.max_retries,
            header_check: config_guard.header_check,
        }
    };

    // Check if we have the response hashed, and if not forward it
    // to the best available RPC.
    //
    // Also handle cache insertions.
    let time = Instant::now();
    (response, rpc_position) = forward_body(tx, &connection_params, cache_args, params).await;

    let time = time.elapsed();
    tracing::info!(?time, "Request time");

    // `rpc_position` is an Option<> that either contains the index of the RPC
    // we forwarded our request to, or is None if the result was cached.
    //
    // Here, we update the latency of the RPC that was used to process the request
    // if `rpc_position` is Some.
    if let Some(rpc_position) = rpc_position {
        update_rpc_latency(&connection_params.rpc_list, rpc_position, time);
    }

    response
}
