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
        selection::{
            cache_rules::has_block_tag,
            select::pick,
        },
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

use blake3::hash;

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
        $cacheable:expr,
        $rpc_id:expr,
        $id:expr,
        $con_params:expr,
        $ttl:expr,
        $max_retries:expr
    ) => {{
        let cached = if $cacheable {
            db_get!($cache_args.cache, $tx_hash.as_bytes().to_owned().into())
        } else {
            Ok(None)
        };
        match cached {
            Ok(Some(mut rax)) => {
                $rpc_id = None;
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
                    $rpc_id,
                    $id,
                    $con_params,
                    $ttl,
                    $max_retries
                )
            }
            Err(_) => {
                // If anything errors send an rpc request and see if it works, if not then gg
                print_cache_error!();
                $rpc_id = None;
                return (cache_error!(), $rpc_id);
            }
        }
    }};
}

macro_rules! fetch_from_rpc {
    (
        $tx:expr,
        $cache_args:expr,
        $tx_hash:expr,
        $rpc_id:expr,
        $id:expr,
        $con_params:expr,
        $ttl:expr,
        $max_retries:expr
    ) => {{
        // Kinda jank but set the id back to what it was before
        $tx["id"] = $id.into();

        // Loop until we get a response
        let rx;
        let mut retries = 0;
        loop {
            // Get the next Rpc in line.
            let mut rpc;
            {
                let mut rpc_list_guard = $con_params.rpc_list.write().unwrap_or_else(|e| {
                    // Handle the case where the RwLock is poisoned
                    e.into_inner()
                });

                let position;
                (rpc, position) = pick(&mut rpc_list_guard);
                $rpc_id = position.map(|_| rpc.id());
            }
            tracing::info!(rpc.name, "Forwarding to");

            // Check if we have any RPCs in the list, if not return error
            if $rpc_id == None {
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
                return (timed_out!(), $rpc_id);
            }
        }

        // Don't cache responses that contain errors or missing trie nodes
        cache_query(&rx, $tx, $tx_hash, &$cache_args).await;

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
    let tx = incoming_to_value(tx).await.unwrap();

    forward_value(tx, con_params, cache_args, params).await
}

/// Answers an already parsed JSON-RPC request, from the cache if possible.
async fn forward_value<K, V>(
    mut tx: Value,
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
    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // Rewrite named block parameters if possible, and only then hash the request:
    // `latest` must be cached under the block it resolved to, not under `latest`.
    let mut tx = replace_block_tags(&mut tx, &cache_args.named_numbers);
    let tx_string = tx.to_string();
    let tx_hash = hash(tx_string.as_bytes());
    let cacheable = !has_block_tag(&tx_string);

    // Id of the RPC used to get the response, we use it to update its latency later.
    let mut rpc_id;

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache_args,
        tx_hash,
        cacheable,
        rpc_id,
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

    (Ok(res), rpc_id)
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
    let rpc_id: Option<usize>;

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
    (response, rpc_id) = forward_body(tx, &connection_params, cache_args, params).await;

    let time = time.elapsed();
    tracing::info!(?time, "Request time");

    // `rpc_id` is an Option<> that either contains the id of the RPC
    // we forwarded our request to, or is None if the result was cached.
    //
    // Here, we update the latency of the RPC that was used to process the request
    // if `rpc_id` is Some.
    if let Some(rpc_id) = rpc_id {
        update_rpc_latency(&connection_params.rpc_list, rpc_id, time);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::accept::db_insert;
    use http_body_util::BodyExt;
    use serde_json::json;
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use tokio::{
        io::{
            AsyncReadExt,
            AsyncWriteExt,
        },
        net::{
            TcpListener,
            TcpStream,
        },
    };

    /// Reads one HTTP request (headers and body) so the response isn't sent early.
    async fn read_request(stream: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = memchr::memmem::find(&buf, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buf[..end]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|len| len.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if buf.len() >= end + 4 + len {
                    return;
                }
            }
        }
    }

    /// An upstream RPC whose result is the number of requests it has received,
    /// so a cache hit can be told apart from a fresh answer.
    async fn counting_upstream() -> (url::Url, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::spawn(async move {
                    read_request(&mut stream).await;
                    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{n:x}"}}"#);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        (url, hits)
    }

    fn connection_params(rpc_list: Vec<Rpc>) -> ConnectionParams {
        let (_finalized_tx, finalized_rx) = watch::channel(0);
        let (incoming_tx, _incoming_rx) = mpsc::unbounded_channel();
        let (_outgoing_tx, outgoing_rx) = broadcast::channel(1);
        ConnectionParams::new(
            &Arc::new(RwLock::new(rpc_list)),
            RequestChannels::new(Arc::new(finalized_rx), incoming_tx, outgoing_rx),
            &Arc::new(SubscriptionData::new()),
            &Arc::new(RwLock::new(Settings::default())),
        )
    }

    fn request_params() -> RequestParams {
        RequestParams {
            ttl: 1000,
            max_retries: 3,
            header_check: false,
        }
    }

    async fn result_of(
        response: (
            Result<hyper::Response<Full<Bytes>>, Infallible>,
            Option<usize>,
        ),
    ) -> Value {
        let body = response.0.unwrap().into_body().collect().await.unwrap();
        let body: Value = serde_json::from_slice(&body.to_bytes()).unwrap();
        body["result"].clone()
    }

    fn get_balance(block: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "eth_getBalance",
            "params": ["0x00000000000000000000000000000000000000aa", block],
        })
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_latest_is_cached_per_block() {
        let (url, hits) = counting_upstream().await;
        let con_params = connection_params(vec![Rpc::new(url, None, 10, 0, 10.0)]);
        let cache_args = CacheArgs::default();
        cache_args.named_numbers.write().unwrap().latest = 0x10;

        let first = forward_value(
            get_balance("latest"),
            &con_params,
            cache_args.clone(),
            request_params(),
        );
        assert_eq!(result_of(first.await).await, "0x1");

        // Same head: the answer for block 0x10 comes from the cache.
        let second = forward_value(
            get_balance("latest"),
            &con_params,
            cache_args.clone(),
            request_params(),
        );
        assert_eq!(result_of(second.await).await, "0x1");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        // The head moved, so `latest` must not be answered with block 0x10's result.
        cache_args.named_numbers.write().unwrap().latest = 0x11;
        let third = forward_value(
            get_balance("latest"),
            &con_params,
            cache_args.clone(),
            request_params(),
        );
        assert_eq!(result_of(third.await).await, "0x2");
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_unresolved_block_tag_skips_cache() {
        let (url, hits) = counting_upstream().await;
        let con_params = connection_params(vec![Rpc::new(url, None, 10, 0, 10.0)]);
        let cache_args = CacheArgs::default();

        // Older versions cached `latest` under the tag itself. With no known head the
        // tag can't be resolved, and such a stale entry must not be served.
        let mut stale_key = get_balance("latest");
        stale_key["id"] = Value::Null;
        drop(
            db_insert(
                &cache_args.cache,
                *hash(stale_key.to_string().as_bytes()).as_bytes(),
                br#"{"jsonrpc":"2.0","id":null,"result":"0xdead"}"#.to_vec(),
            )
            .await,
        );

        let response = forward_value(
            get_balance("latest"),
            &con_params,
            cache_args.clone(),
            request_params(),
        );
        assert_eq!(result_of(response.await).await, "0x1");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }
}
