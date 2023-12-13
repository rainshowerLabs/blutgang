use crate::{
    balancer::{
        format::{
            incoming_to_value,
            replace_block_tags,
        },
        processing::{
            cache_querry,
            CacheArgs,
        },
        selection::select::pick,
    },
    cache_error,
    no_rpc_available,
    print_cache_error,
    rpc::types::Rpc,
    rpc_response,
    timed_out,
    websocket::{
        server::serve_websocket,
        types::RequestResult,
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

use dashmap::DashMap;

use serde_json::Value;
use simd_json;

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

use sled::Db;

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

struct RequestParams {
    ttl: u128,
    max_retries: u32,
}

#[derive(Debug)]
pub struct RequestChannels {
    pub finalized_rx: Arc<watch::Receiver<u64>>,
    pub incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    pub outgoing_rx: broadcast::Receiver<Value>,
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

// Macros for accepting requests
#[macro_export]
macro_rules! accept {
    (
        $io:expr,
        $rpc_list_rwlock:expr,
        $cache:expr,
        $channels:expr,
        $named_numbers:expr,
        $head_cache:expr,
        $sink_map:expr,
        $subscribed_users:expr,
        $config:expr
    ) => {
        // Bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        $channels,
                        $named_numbers,
                        $head_cache,
                        $sink_map,
                        $subscribed_users,
                        Arc::clone($cache),
                        $config,
                    );
                    response
                }),
            )
            .with_upgrades()
            .await
        {
            println!("\x1b[31mErr:\x1b[0m Error serving connection: {:?}", err);
        }
    };
}

// Macro for getting responses from either the cache or RPC nodes
macro_rules! get_response {
    (
        $tx:expr,
        $cache:expr,
        $tx_hash:expr,
        $rpc_position:expr,
        $id:expr,
        $rpc_list_rwlock:expr,
        $finalized_rx:expr,
        $named_numbers:expr,
        $head_cache:expr,
        $ttl:expr,
        $max_retries:expr
    ) => {
        match $cache.get($tx_hash.as_bytes()) {
            Ok(Some(mut rax)) => {
                $rpc_position = None;
                // Reconstruct ID
                let mut cached: Value = simd_json::serde::from_slice(&mut rax).unwrap();

                cached["id"] = $id.into();
                cached.to_string()
            },
            Ok(None) => {
                // Kinda jank but set the id back to what it was before
                $tx["id"] = $id.into();

                // Loop until we get a response
                let mut rx;
                let mut retries = 0;
                loop {
                    // Get the next Rpc in line.
                    let mut rpc;
                    {
                        let mut rpc_list = $rpc_list_rwlock.write().unwrap();
                        (rpc, $rpc_position) = pick(&mut rpc_list);
                    }
                    println!("\x1b[35mInfo:\x1b[0m Forwarding to: {}", rpc.url);

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
                        },
                        Err(_) => {
                            println!("\x1b[93mWrn:\x1b[0m An RPC request has timed out, picking new RPC and retrying.");
                            rpc.update_latency($ttl as f64);
                            retries += 1;
                        },
                    };

                    if retries == $max_retries {
                        return (timed_out!(), $rpc_position,);
                    }
                }

                let cache_args = CacheArgs {
                    finalized_rx: $finalized_rx,
                    named_numbers: $named_numbers,
                    cache: $cache,
                    head_cache: $head_cache,
                    subscribed_users: None,
                };

                // Don't cache responses that contain errors or missing trie nodes
                cache_querry(
                    &mut rx,
                    $tx,
                    $tx_hash,
                    &cache_args,
                );

                rx
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

// Pick RPC and send request to it. In case the result is cached,
// read and return from the cache.
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    finalized_rx: &watch::Receiver<u64>,
    named_numbers: &Arc<RwLock<NamedBlocknumbers>>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    cache: Arc<Db>,
    params: RequestParams,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
) {
    // Check if body has application/json
    if tx.headers().get("content-type") != Some(&HeaderValue::from_static("application/json")) {
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
    let mut tx = replace_block_tags(&mut tx, named_numbers);

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache,
        tx_hash,
        rpc_position,
        id,
        rpc_list_rwlock,
        finalized_rx.clone(),
        named_numbers.clone(),
        head_cache.clone(),
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

// Forward the request to *a* RPC picked by the algo set by the user.
// Measures the time needed for a request, and updates the respective
// RPC lself.
// In case of a timeout, returns an error.
pub async fn accept_request(
    mut tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    channels: RequestChannels,
    named_numbers: &Arc<RwLock<NamedBlocknumbers>>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    sink_map: &Arc<DashMap<u64, mpsc::UnboundedSender<RequestResult>>>,
    subscribed_users: &Arc<DashMap<u64, DashMap<u64, bool>>>,
    cache: Arc<Db>,
    config: &Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Check if the request is a websocket upgrade request.
    if is_upgrade_request(&tx) {
        println!("\x1b[35mInfo:\x1b[0m Received WS upgrade request");

        let (response, websocket) = match upgrade(&mut tx, None) {
            Ok((response, websocket)) => (response, websocket),
            Err(e) => {
                println!("\x1b[31mErr:\x1b[0m Websocket upgrade error: {e}");
                return rpc_response!(500, Full::new(Bytes::from(
                    "{code:-32004, message:\"error: Websocket upgrade error! Try again later...\"}"
                        .to_string(),
                )));
            }
        };

        let cache_args = CacheArgs {
            finalized_rx: channels.finalized_rx.as_ref().clone(),
            named_numbers: named_numbers.clone(),
            cache,
            head_cache: head_cache.clone(),
            subscribed_users: Some(subscribed_users.clone()),
        };

        // Spawn a task to handle the websocket connection.
        let sink_clone = sink_map.clone();
        tokio::task::spawn(async move {
            if let Err(e) = serve_websocket(
                websocket,
                channels.incoming_tx,
                channels.outgoing_rx,
                sink_clone,
                cache_args,
            )
            .await
            {
                println!("\x1b[31mErr:\x1b[0m Websocket connection error: {e}");
            }
        });

        // Return the response so the spawned future can continue.
        return Ok(response);
    }

    // Send request and measure time
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    let rpc_position: Option<usize>;

    // RequestParams from config
    let params = {
        let config_guard = config.read().unwrap();
        RequestParams {
            ttl: config_guard.ttl,
            max_retries: config_guard.max_retries,
        }
    };

    // Check if we have the response hashed, and if not forward it
    // to the best available RPC.
    //
    // Also handle cache insertions.
    let time = Instant::now();
    (response, rpc_position) = forward_body(
        tx,
        &rpc_list_rwlock,
        &channels.finalized_rx,
        named_numbers,
        head_cache,
        cache,
        params,
    )
    .await;
    let time = time.elapsed();
    println!("\x1b[35mInfo:\x1b[0m Request time: {:?}", time);

    // `rpc_position` is an Option<> that either contains the index of the RPC
    // we forwarded our request to, or is None if the result was cached.
    //
    // Here, we update the latency of the RPC that was used to process the request
    // if `rpc_position` is Some.
    if let Some(rpc_position) = rpc_position {
        let mut rpc_list_guard = rpc_list_rwlock.write().unwrap_or_else(|e| {
            // Handle the case where the RwLock is poisoned
            e.into_inner()
        });

        // Handle weird edge cases ¯\_(ツ)_/¯
        if rpc_list_guard.is_empty() {
            println!("LA {}", rpc_list_guard[rpc_position].status.latency);
        } else {
            let index = if rpc_position >= rpc_list_guard.len() {
                rpc_list_guard.len() - 1
            } else {
                rpc_position
            };
            rpc_list_guard[index].update_latency(time.as_nanos() as f64);
            rpc_list_guard[index].last_used = time.as_micros();
            println!("LA {}", rpc_list_guard[index].status.latency);
        }
    }

    response
}
