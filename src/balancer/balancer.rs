use crate::{
    balancer::format::incoming_to_value,
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
    print,
    str::from_utf8,
    sync::{
        Arc,
        Mutex,
    },
    time::Instant,
};

// TODO: Since we're not ranking RPCs properly, just pick the next one in line for now
fn pick(
    list: &Vec<Rpc>,
) -> (Rpc, usize) {
    let mut lowest = f64::MAX;
    let mut fallback_latency = f64::MAX; // Second lowest latency
    let mut now = 0;
    let mut fallback = 0; // second lowest index
    
    // Find the RPC with the lowest average latency from the Vec
    for i in 0..list.len() {
        if list[i].status.latency < lowest {
            fallback_latency = lowest;
            fallback = now;

            lowest = list[i].status.latency;
            now = i;
        }
    }

    if list[now].max_consecutive <= list[now].consecutive {
        list[fallback].consecutive = 1;
        return (list[fallback], fallback);
    }

    list[now].consecutive += 1;
    (list[now], now)
}

async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc: Rpc,
    cache: Arc<Db>,
) -> (Result<hyper::Response<Full<Bytes>>, Infallible>, bool) {
    println!("Forwarding to: {}", rpc.url);
    // Convert incoming body to serde value
    let tx = incoming_to_value(tx).await.unwrap();

    // Flag that lets us know if we used the cache so we know if to rank the rpcs
    let mut cache_hit = false;

    // Check if `tx` contains latest anywhere. If not, write or retrieve it from the db
    // TODO: make this faster
    let rax;
    let tx_string = format!("{}", tx);

    if tx_string.contains("latest") || tx_string.contains("blockNumber") {
        rax = rpc.send_request(tx).await.unwrap();
    } else {
        let tx_hash = hash(tx_string.as_bytes());

        rax = match cache.get(*tx_hash.as_bytes()) {
            Ok(rax) => {
                // TODO: This is poverty
                if let Some(rax) = rax {
                    cache_hit = true;
                    from_utf8(&rax).unwrap().to_string()
                } else {
                    let rx = rpc.send_request(tx.clone()).await.unwrap();
                    let rx_str = rx.as_str().to_string();

                    // Don't cache responses that contain errors or missing trie nodes
                    if !rx_str.contains("missing") || !rx_str.contains("error") {
                        cache.insert(*tx_hash.as_bytes(), rx.as_bytes()).unwrap();
                    }

                    rx_str
                }
            }
            Err(_) => {
                // If anything errors send an rpc request and see if it works, if not then gg
                print!("oopsies error!");
                let rx = rpc.send_request(tx.clone()).await.unwrap();
                // cache.insert(tx_hash_bytes, rx.as_bytes()).unwrap();
                let rx_str = rx.as_str().to_string();
                rx_str
            }
        };
    }

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    //Build the response
    let res = hyper::Response::builder().status(200).body(body).unwrap();

    (Ok(res), cache_hit)
}

pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_mtx: Arc<Mutex<Vec<Rpc>>>,
    last_mtx: Arc<Mutex<usize>>,
    ma_lenght: f64,
    cache: Arc<Db>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Get the next Rpc in line
    let rpc;
    let now;
    {
        let mut last = last_mtx.lock().unwrap();
        let rpc_list = rpc_list_mtx.lock().unwrap();

        (rpc, now) = pick(&rpc_list);
        *last += now;
    }

    println!("LB {}", rpc.status.latency);

    // Send request and measure time
    let time = Instant::now();
    let response;
    let hit_cache;
    (response, hit_cache) = forward_body(tx, rpc, cache).await;
    let time = time.elapsed();

    println!("Request time: {:?}", time);
    // Get lock for the rpc list and add it to the moving average
    if !hit_cache {
        let mut rpc_list = rpc_list_mtx.lock().unwrap();
        rpc_list[now].update_latency(time.as_nanos() as f64, ma_lenght);
        println!("LA {}", rpc_list[now].status.latency);
    }

    response
}
