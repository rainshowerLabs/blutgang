use crate::{
    balancer::format::{
        get_block_number_from_request,
        incoming_to_value,
    },
    balancer::selection::cache_rules::{
        cache_method,
        cache_result,
    },
    balancer::selection::selection::pick,
    rpc::types::Rpc,
};

use serde_json::to_vec;

use blake3::hash;
use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};
use sled::Db;
use tokio::time::timeout;

use memchr::memmem;

use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
    time::{
        Duration,
        Instant,
    }, println,
};

// Macros for accepting requests
#[macro_export]
macro_rules! accept {
    ($io:expr, $rpc_list_rwlock:expr, $ma_length:expr, $cache:expr, $finalized_rx:expr, $head_cache:expr, $ttl:expr) => {
        // Finally, we bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = accept_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        $finalized_rx,
                        $head_cache,
                        Arc::clone($cache),
                        $ttl,
                    );
                    response
                }),
            )
            .await
        {
            println!("error serving connection: {:?}", err);
        }
    };
}

// Macro for getting responses from either the cache or RPC nodes
macro_rules! get_response {
    ($tx:expr, $cache:expr, $tx_hash:expr, $rpc_position:expr, $id:expr, $rpc_list_rwlock:expr, $finalized_rx:expr, $head_cache:expr, $ttl:expr) => {
        match $cache.get($tx_hash.as_bytes()) {
            Ok(rax) => {
                if let Some(rax) = rax {
                    $rpc_position = None;

                    // Reconstruct ID
                    let mut cached: serde_json::Value = serde_json::from_slice(&rax).unwrap();
                    cached["id"] = $id.into();
                    cached.to_string()
                } else {
                    // Kinda jank but set the id back to what it was before
                    $tx["id"] = $id.into();

                    let tx_string = $tx.to_string();

                    // Quit blutgang if `tx_string` contains the word `blutgang_quit`
                    // Only for debugging, remove this for production builds.
                    if memmem::find(&tx_string.as_bytes(), "blutgang_quit".as_bytes()).is_some() {
                        std::process::exit(0);
                    }
                    if memmem::find(&tx_string.as_bytes(), "blutgang_is_lb".as_bytes()).is_some() {
                        return (
                            Ok(hyper::Response::builder()
                                .status(200)
                                .body(Full::new(Bytes::from(
                                    "{message: \"blutgang v0.1.1 nc\"}".to_string(),
                                )))
                                .unwrap()),
                            None,
                        );
                    }

                    // Loop until we get a response
                    let rx;
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
                            return (
                                Ok(hyper::Response::builder()
                                    .status(500)
                                    .body(Full::new(Bytes::from(
                                        "{code:-32002, message:\"error: No working RPC available! Try again later...\"}".to_string(),
                                    )))
                                    .unwrap()),
                                None,
                            );
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
                                rpc.update_latency($ttl as f64);
                                retries += 1;
                            },
                        };

                        if retries == 128 {
                            return (
                                Ok(hyper::Response::builder()
                                    .status(408)
                                    .body(Full::new(Bytes::from(
                                        "{code:-32001, message:\"error: Request timed out! Try again later...\"".to_string(),
                                    )))
                                    .unwrap()),
                                $rpc_position,
                            );
                        }

                    }

                    let rx_str = rx.as_str().to_string();

                    // Don't cache responses that contain errors or missing trie nodes
                    if cache_method(&tx_string) && cache_result(&rx) {
                        // Insert the response hash into the head_cache
                        let num = get_block_number_from_request($tx);
                        if num.is_some() {
                            let num = num.unwrap();

                            if num > *$finalized_rx.borrow() {
                                let mut head_cache = $head_cache.write().unwrap();
                                head_cache
                                    .entry(num)
                                    .or_insert_with(Vec::new)
                                    .push($tx_hash.to_string());

                            }

                            // Replace the id with 0 and insert that
                            let mut rx_value: serde_json::Value =
                                serde_json::from_str(&rx_str).unwrap();
                            rx_value["id"] = serde_json::Value::Null;

                            $cache.insert($tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice()).unwrap();
                        }

                    }

                    rx_str
                }
            }
            Err(_) => {
                // If anything errors send an rpc request and see if it works, if not then gg
                println!("\x1b[31m!!! Cache error! Check the DB !!!\x1b[0m");
                println!("To recover, please stop blutgang, delete your cache folder, and start blutgang again.");
                println!("If the error perists, please open up an issue: https://github.com/rainshowerLabs/blutgang/issues");
                $rpc_position = None;
                return (
                    Ok(hyper::Response::builder()
                        .status(500)
                        .body(Full::new(Bytes::from(
                            "{code:-32003, message:\"error: Cache error! Try again later...\"".to_string(),
                        )))
                        .unwrap()),
                    $rpc_position,
                );
            }
        }
    };
}

// Pick RPC and send request to it. In case the result is cached,
// read and return from the cache.
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    finalized_rx: &tokio::sync::watch::Receiver<u64>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    cache: Arc<Db>,
    ttl: u128,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
) {
    // Convert incoming body to serde value
    let mut tx = incoming_to_value(tx).await.unwrap();

    // Get the id of the request and set it to 0 for caching
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // read tx as bytes
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());

    // TODO: Current RPC position. idk of a better way for now so this will do
    let mut rpc_position;

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache,
        tx_hash,
        rpc_position,
        id,
        rpc_list_rwlock,
        finalized_rx,
        head_cache,
        ttl
    );

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    //Build the response
    let res = hyper::Response::builder().status(200).body(body).unwrap();

    (Ok(res), rpc_position)
}

// Forward the request to *a* RPC picked by the algo set by the user.
// Measures the time needed for a request, and updates the respective
// RPC lself.
// In case of a timeout, returns an error.
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    finalized_rx: &tokio::sync::watch::Receiver<u64>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    cache: Arc<Db>,
    ttl: u128,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Send request and measure time
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    let rpc_position: Option<usize>;

    let time = Instant::now();
    (response, rpc_position) =
        forward_body(tx, &rpc_list_rwlock, finalized_rx, head_cache, cache, ttl).await;
    let time = time.elapsed();
    println!("Request time: {:?}", time);

    // Get lock for the rpc list and add it to the moving average if we picked an rpc
    if rpc_position.is_some() {
        let mut rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

        if rpc_list_rwlock_guard.len() == 0 {
            return response;
        }

        if rpc_list_rwlock_guard.len() == 1 {
            rpc_list_rwlock_guard[0].update_latency(time.as_nanos() as f64);
        } else {
            rpc_list_rwlock_guard[rpc_position.unwrap()].update_latency(time.as_nanos() as f64);
        }

        println!(
            "LA {}",
            rpc_list_rwlock_guard[rpc_position.unwrap()].status.latency
        );
    }

    response
}
