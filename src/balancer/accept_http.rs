use crate::{
    balancer::{
        format::{
            incoming_to_value,
            replace_block_tags,
        },
        processing::{
            cache_querry,
            update_rpc_latency,
            CacheArgs,
        },
        selection::select::pick,
    },
    cache_error,
    database::types::RequestBus,
    log_err,
    log_info,
    log_wrn,
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
    NamedBlocknumbers,
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

use sled::{
    Db,
    InlineArray,
};

use tokio::time::timeout;

use std::{
    collections::BTreeMap,
    convert::Infallible,
    println,
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
#[derive(Debug, Clone)]
pub struct ConnectionParams<K, V>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    channels: RequestChannels,
    named_numbers: Arc<RwLock<NamedBlocknumbers>>,
    head_cache: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    sub_data: Arc<SubscriptionData>,
    cache: RequestBus<K, V>,
    config: Arc<RwLock<Settings>>,
}

impl<K, V> ConnectionParams<K, V>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    pub fn new(
        rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
        channels: RequestChannels,
        named_numbers: &Arc<RwLock<NamedBlocknumbers>>,
        head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
        sub_data: &Arc<SubscriptionData>,
        cache: &RequestBus<K, V>,
        config: &Arc<RwLock<Settings>>,
    ) -> Self {
        ConnectionParams {
            rpc_list: rpc_list_rwlock.clone(),
            channels,
            named_numbers: named_numbers.clone(),
            head_cache: head_cache.clone(),
            sub_data: sub_data.clone(),
            cache: cache.clone(),
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
        $connection_params:expr
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_request(req, $connection_params);
                    response
                }),
            )
            .with_upgrades()
            .await
        {
            log_err!("Error serving connection: {:?}", err);
        }
    };
}

/// Pick RPC and send request to it. In case the result is cached,
/// read and return from the cache.
pub async fn forward_body<K, V>(
    tx: Request<hyper::body::Incoming>,
    con_params: &ConnectionParams<K, V>,
    cache_args: &CacheArgs,
    params: RequestParams,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
)
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
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
    let tx = incoming_to_value(tx).await.unwrap();

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let (rax, position) = get_response(tx, con_params, cache_args, params)
        .await
        .unwrap();

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

    (Ok(res), position)
}

/// Forward the request to *a* RPC picked by the algo set by the user.
/// Measures the time needed for a request, and updates the respective
/// RPC lself.
/// In case of a timeout, returns an error.
pub async fn accept_request<K, V>(
    mut tx: Request<hyper::body::Incoming>,
    connection_params: ConnectionParams<K, V>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible>
where
    K: AsRef<[u8]>,
    V: Into<InlineArray>,
{
    // Check if the request is a websocket upgrade request.
    if is_upgrade_request(&tx) {
        log_info!("Received WS upgrade request");

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
                log_err!("Websocket upgrade error: {}", e);
                return rpc_response!(500, Full::new(Bytes::from(
                    "{code:-32004, message:\"error: Websocket upgrade error! Try again later...\"}"
                        .to_string(),
                )));
            }
        };

        let cache_args = CacheArgs {
            finalized_rx: connection_params.channels.finalized_rx.as_ref().clone(),
            named_numbers: connection_params.named_numbers.clone(),
            cache: connection_params.cache,
            head_cache: connection_params.head_cache.clone(),
        };

        // Spawn a task to handle the websocket connection.
        tokio::task::spawn(async move {
            if let Err(e) = serve_websocket(
                websocket,
                connection_params.channels.incoming_tx,
                connection_params.channels.outgoing_rx,
                connection_params.sub_data.clone(),
                cache_args,
            )
            .await
            {
                log_err!("Websocket connection error: {}", e);
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
    (response, rpc_position) = forward_body(
        tx,
        &connection_params.rpc_list_rwlock,
        &connection_params.channels.finalized_rx,
        &connection_params.named_numbers,
        &connection_params.head_cache,
        connection_params.cache,
        params,
    )
    .await;

    response
}
