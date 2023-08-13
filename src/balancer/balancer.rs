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
fn pick(list: &Vec<Rpc>, last: usize) -> (Rpc, usize) {
    let now = last + 1;
    if now >= list.len() {
        return (list[last].clone(), 0);
    }
    (list[last].clone(), now)
}

async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc: Rpc,
    cache: Arc<Db>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    println!("Forwarding to: {}", rpc.url);
    // Convert incoming body to serde value
    let tx = incoming_to_value(tx).await.unwrap();

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

    Ok(res)
}

pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_mtx: Arc<Mutex<Vec<Rpc>>>,
    last_mtx: Arc<Mutex<usize>>,
    cache: Arc<Db>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Get the next Rpc in line
    let rpc;
    {
        let mut last = last_mtx.lock().unwrap();
        let rpc_list = rpc_list_mtx.lock().unwrap();

        let now;
        (rpc, now) = pick(&rpc_list, *last);
        *last = now;
    }

    let time = Instant::now();
    let response = forward_body(tx, rpc, cache).await;
    let time = time.elapsed();
    println!("Request time: {:?}", time);
    
    response
}
