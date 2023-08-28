use crate::{
    balancer::format::incoming_to_value,
    balancer::selection::cache_rules::cache_method,
    balancer::selection::selection::pick,
    rpc::types::Rpc,
};

use blake3::hash;
use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};
use sled::Db;

use std::{
    convert::Infallible,
    str::from_utf8,
    sync::{
        Arc,
        Mutex,
        RwLock,
    },
    time::Instant,
};

// Macros for accepting requests
#[cfg(not(feature = "tui"))]
#[macro_export]
macro_rules! accept {
    ($io:expr, $rpc_list_rwlock:expr, $last_mtx:expr, $ma_lenght:expr, $cache:expr) => {
        // Finally, we bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(move |req| {
                    let response = accept_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        Arc::clone($last_mtx),
                        $ma_lenght,
                        Arc::clone($cache),
                    );
                    response
                }),
            )
            .await
        {
            println!("Error serving connection: {:?}", err);
        }
    };
}

#[cfg(feature = "tui")]
#[macro_export]
macro_rules! accept {
    ($io:expr, $rpc_list_rwlock:expr, $last_mtx:expr, $ma_lenght:expr, $cache:expr, $rx:expr) => {
        // Finally, we bind the incoming connection to our service
        if let Err(err) = http1::Builder::new()
            // `service_fn` converts our function in a `Service`
            .serve_connection(
                $io,
                service_fn(move |req| {
                    let response = accept_request(
                        req,
                        Arc::clone($rpc_list_rwlock),
                        Arc::clone($last_mtx),
                        $ma_lenght,
                        Arc::clone($cache),
                        Arc::clone($rx),
                    );
                    response
                }),
            )
            .await
        {
            println!("Error serving connection: {:?}", err);
        }
    };
}

async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    last_mtx: &Arc<Mutex<usize>>,
    cache: Arc<Db>,
) -> (Result<hyper::Response<Full<Bytes>>, Infallible>, bool) {
    // Convert incoming body to serde value
    let tx = incoming_to_value(tx).await.unwrap();

    // Flag that lets us know if we used the cache so we know if to rank the rpcs
    let mut cache_hit = false;

    // Check if `tx` contains latest anywhere. If not, write or retrieve it from the db
    // TODO: This is lazy and suboptimal
    let rax;
    let tx_string = format!("{}", tx);

    let tx_hash = hash(tx_string.as_bytes());

    // TODO: This is poverty and can be made to be like 2x faster but this is an alpha and idc that much at this point
    rax = match cache.get(*tx_hash.as_bytes()) {
        Ok(rax) => {
            if let Some(rax) = rax {
                cache_hit = true;
                from_utf8(&rax).unwrap().to_string()
            } else {
                // Quit blutgang if `tx_string` contains the word `blutgang_quit`
                // Only for debugging, remove this for production builds.
                if tx_string.contains("blutgang_quit") {
                    std::process::exit(0);
                }

                // Get the next Rpc in line.
                let rpc;
                let now;
                {
                    let mut last = last_mtx.lock().unwrap();
                    let mut rpc_list = rpc_list_rwlock.write().unwrap();

                    (rpc, now) = pick(&mut rpc_list);
                    *last = now;
                }
                #[cfg(not(feature = "tui"))]
                println!("Forwarding to: {}", rpc.url);

                // Send the request.
                //
                // Check if it contains any errors or if its `latest` and insert it if it isn't
                let rx = rpc.send_request(tx.clone()).await.unwrap();
                let rx_str = rx.as_str().to_string();

                // Don't cache responses that contain errors or missing trie nodes
                if cache_method(&tx_string) {
                    cache.insert(*tx_hash.as_bytes(), rx.as_bytes()).unwrap();
                }

                rx_str
            }
        }
        Err(_) => {
            // If anything errors send an rpc request and see if it works, if not then gg
            println!("Cache error! Check the DB!");
            "".to_string()
        }
    };

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    //Build the response
    let res = hyper::Response::builder().status(200).body(body).unwrap();

    (Ok(res), cache_hit)
}

#[cfg(not(feature = "tui"))]
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    last_mtx: Arc<Mutex<usize>>,
    ma_lenght: f64,
    cache: Arc<Db>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Send request and measure time
    let time = Instant::now();
    let response;
    let hit_cache;
    (response, hit_cache) = forward_body(tx, &rpc_list_rwlock, &last_mtx, cache).await;
    let time = time.elapsed();

    println!("Request time: {:?}", time);
    // Get lock for the rpc list and add it to the moving average
    if !hit_cache {
        let mut rpc_list = rpc_list_rwlock.write().unwrap();
        let last = last_mtx.lock().unwrap();

        rpc_list[*last].update_latency(time.as_nanos() as f64, ma_lenght);
        #[cfg(not(feature = "tui"))]
        println!("LA {}", rpc_list[*last].status.latency);
    }

    response
}

#[cfg(feature = "tui")]
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    last_mtx: Arc<Mutex<usize>>,
    ma_lenght: f64,
    cache: Arc<Db>,
    response_list: Arc<RwLock<Vec<String>>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Push request to the response_list
    let tx_string = format!("{:?}", tx);
    // Send request and measure time
    let time = Instant::now();
    let response;
    let hit_cache;
    (response, hit_cache) = forward_body(tx, &rpc_list_rwlock, &last_mtx, cache).await;
    let time = time.elapsed();

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

    // Get lock for the rpc list and add it to the moving average
    if !hit_cache {
        let mut rpc_list = rpc_list_rwlock.write().unwrap();
        let last = last_mtx.lock().unwrap();

        rpc_list[*last].update_latency(time.as_nanos() as f64, ma_lenght);
        #[cfg(not(feature = "tui"))]
        println!("LA {}", rpc_list[*last].status.latency);
    }

    response
}
