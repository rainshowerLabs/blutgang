use crate::{
    balancer::format::incoming_to_value,
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

// Macros for accepting requests
#[macro_export]
macro_rules! accept {
    ($io:expr, $rpc_list_rwlock:expr, $ma_length:expr, $cache:expr, $ttl:expr) => {
        // Finally, we bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(move |req| {
                    let response = accept_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        $ma_length,
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
    ($tx:expr, $cache:expr, $tx_hash:expr, $rpc_position:expr, $id:expr, $rpc_list_rwlock:expr, $ttl:expr) => {
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

                    // Get the next Rpc in line.
                    let rpc;
                    {
                        let mut rpc_list = $rpc_list_rwlock.write().unwrap();
                        (rpc, $rpc_position) = pick(&mut rpc_list);
                    }
                    println!("Forwarding to: {}", rpc.url);

                    // Check if we have any RPCs in the list, if not return error
                    if $rpc_position == None {
                        return (
                            Ok(hyper::Response::builder()
                                .status(500)
                                .body(Full::new(Bytes::from(
                                    "{code:-32002, message:\"error: No working RPC available! Try again later...\"".to_string(),
                                )))
                                .unwrap()),
                            None,
                        );
                    }

                    // Send the request. And return a timeout if it takes too long
                    //
                    // Check if it contains any errors or if its `latest` and insert it if it isn't
                    let rx = match timeout(
                        Duration::from_millis($ttl.try_into().unwrap()),
                        rpc.send_request($tx.clone()),
                    )
                    .await
                    {
                        Ok(rx) => rx.unwrap(),
                        Err(_) => {
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
                    };

                    let rx_str = rx.as_str().to_string();

                    // Don't cache responses that contain errors or missing trie nodes
                    if cache_method(&tx_string) && cache_result(&rx) {
                        // Replace the id with 0 and insert that
                        let mut rx_value: serde_json::Value =
                            serde_json::from_str(&rx_str).unwrap();
                        rx_value["id"] = serde_json::Value::Null;

                        $cache.insert($tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice()).unwrap();
                    }

                    rx_str
                }
            }
            Err(_) => {
                // If anything errors send an rpc request and see if it works, if not then gg
                println!("!!! Cache error! Check the DB !!!");
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
    let rpc_position;

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    // TODO: This is poverty and can be made to be like 2x faster but this is an alpha and idc that much at this point
    let rax = get_response!(tx, cache, tx_hash, rpc_position, id, rpc_list_rwlock, ttl);

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
    ma_length: f64,
    cache: Arc<Db>,
    ttl: u128,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Send request and measure time
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    let rpc_position: Option<usize>;

    // TODO: make this timeout mechanism more robust. if an rpc times out, remove it from the active pool and pick a new one.
    let time = Instant::now();
    (response, rpc_position) = forward_body(tx, &rpc_list_rwlock, cache, ttl).await;
    let time = time.elapsed();
    println!("Request time: {:?}", time);

    // Get lock for the rpc list and add it to the moving average if we picked an rpc
    if rpc_position.is_some() {
        let mut rpc_list_rwlock_guard = rpc_list_rwlock.write().unwrap();

        rpc_list_rwlock_guard[rpc_position.unwrap()]
            .update_latency(time.as_nanos() as f64, ma_length);
        println!(
            "LA {}",
            rpc_list_rwlock_guard[rpc_position.unwrap()].status.latency
        );
    }

    response
}
