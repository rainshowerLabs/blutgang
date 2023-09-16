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

use std::{
    convert::Infallible,
    str::from_utf8,
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
#[cfg(not(feature = "tui"))]
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

#[cfg(feature = "tui")]
#[macro_export]
macro_rules! accept {
    ($io:expr, $rpc_list_rwlock:expr, $ma_length:expr, $cache:expr, $rx:expr, $ttl:expr) => {
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
                        Arc::clone($rx),
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

// Pick RPC and send request to it. In case the result is cached,
// read and return from the cache.
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
) {
    // Convert incoming body to serde value
    let tx = incoming_to_value(tx).await.unwrap();

    // read tx as bytes
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());

    // TODO: Current RPC position. idk of a better way for now so this will do
    let rpc_position;

    // Check if `tx` contains latest anywhere. If not, write or retrieve it from the db
    // TODO: This is poverty and can be made to be like 2x faster but this is an alpha and idc that much at this point
    let rax;
    rax = match cache.get(*tx_hash.as_bytes()) {
        Ok(rax) => {
            if let Some(rax) = rax {
                rpc_position = None;
                from_utf8(&rax).unwrap().to_string()
            } else {
                let tx_string = tx.to_string();

                // Quit blutgang if `tx_string` contains the word `blutgang_quit`
                // Only for debugging, remove this for production builds.
                if tx_string.contains("blutgang_quit") {
                    std::process::exit(0);
                }

                // Get the next Rpc in line.
                let rpc;
                {
                    let mut rpc_list = rpc_list_rwlock.write().unwrap();
                    (rpc, rpc_position) = pick(&mut rpc_list);
                }
                #[cfg(not(feature = "tui"))]
                println!("Forwarding to: {}", rpc.url);

                // Check if we have any RPCs in the list, if not return error
                if rpc_position == None {
                    return (
                        Ok(hyper::Response::builder()
                            .status(200)
                            .body(Full::new(Bytes::from(
                                "error: No working RPC available!".to_string(),
                            )))
                            .unwrap()),
                        None,
                    );
                }

                // Send the request.
                //
                // Check if it contains any errors or if its `latest` and insert it if it isn't
                let rx = rpc.send_request(tx.clone()).await.unwrap();
                let rx_str = rx.as_str().to_string();

                // Don't cache responses that contain errors or missing trie nodes
                if cache_method(&tx_string) || cache_result(&rx) {
                    cache.insert(*tx_hash.as_bytes(), rx.as_bytes()).unwrap();
                }

                rx_str
            }
        }
        Err(_) => {
            // If anything errors send an rpc request and see if it works, if not then gg
            println!("Cache error! Check the DB!");
            rpc_position = None;
            "".to_string()
        }
    };

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
#[cfg(not(feature = "tui"))]
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    ma_length: f64,
    cache: Arc<Db>,
    ttl: u128,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Send request and measure time
    let time = Instant::now();
    let response;
    let mut rpc_position: Option<usize> = Default::default();

    // TODO: make this timeout mechanism more robust. if an rpc times out, remove it from the active pool and pick a new one.
    let future = forward_body(tx, &rpc_list_rwlock, cache);
    let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), future).await;
    let time = time.elapsed();

    match result {
        Ok((res, now)) => {
            response = res;
            rpc_position = now;
        }
        Err(_) => {
            response = Ok(hyper::Response::builder()
                .status(200)
                .body(Full::new(Bytes::from(
                    "error: Request timed out! Try again later...".to_string(),
                )))
                .unwrap());
        }
    }

    println!("Request time: {:?}", time);
    // Get lock for the rpc list and add it to the moving average if we picked an rpc
    if rpc_position != None {
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

#[cfg(feature = "tui")]
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    ma_length: f64,
    cache: Arc<Db>,
    response_list: Arc<RwLock<Vec<String>>>,
    ttl: u128,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Send request and measure time
    let time = Instant::now();
    let response;
    let mut rpc_position: Option<usize> = Default::default();

    let tx_string = format!("{:?}", tx);

    // TODO: make this timeout mechanism more robust. if an rpc times out, remove it from the active pool and pick a new one.
    let future = forward_body(tx, &rpc_list_rwlock, cache);
    let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), future).await;
    let time = time.elapsed();

    match result {
        Ok((res, now)) => {
            response = res;
            rpc_position = now;
        }
        Err(_) => {
            response = Ok(hyper::Response::builder()
                .status(200)
                .body(Full::new(Bytes::from(
                    "error: Request timed out! Try again later...".to_string(),
                )))
                .unwrap());
        }
    }

    // Convert response to string and send to response_list
    //
    // If the Vec has 8192 elements, remove the oldest one
    let response_string = format!("{:?}", &response.as_ref().unwrap());
    {
        let mut response_list = response_list.write().unwrap();
        if response_list.len() == 8190 {
            response_list.remove(0);
        }
        response_list.push(tx_string);
        response_list.push(response_string);
        response_list.push("".to_string());
    }

    println!("Request time: {:?}", time);
    // Get lock for the rpc list and add it to the moving average if we picked an rpc
    if rpc_position != None {
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
