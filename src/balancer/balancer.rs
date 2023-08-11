use crate::{
	rpc::types::Rpc,
	balancer::format::incoming_to_value,
};

use blake3::hash;
use http_body_util::Full;
use hyper::{
	Request,
	body::Bytes,
};
use sled::Db;

use std::{
	sync::{
    	Arc,
    	Mutex,
	},
	str::from_utf8,
	convert::Infallible,
};

// TODO: Since we're not ranking RPCs properly, just pick the next one in line for now
fn pick(list: &Vec<Rpc>, last: usize) -> (Rpc, usize) {
    println!("{:?}", last);
    println!("{:?}", list.len());
    let now = last + 1;
    if now >= list.len() {
        return (list[last].clone(), 0);
    }
    (list[last].clone(), now)
}

pub async fn forward(
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

        println!("last: {:?}", last);
        let now;
        (rpc, now) = pick(&rpc_list, *last);
        *last = now;
    }

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
        let tx_hash_bytes: [u8; 32] = *tx_hash.as_bytes();

        rax = match cache.get(tx_hash_bytes) {
            Ok(rax) => {
                // TODO: This is poverty
                if let Some(rax) = rax {
                    from_utf8(&rax).unwrap().to_string()
                } else {
                    let rx = rpc.send_request(tx.clone()).await.unwrap();
                    let rx_str = rx.as_str().to_string();

                    // Don't cache responses that contain errors or missing trie nodes
                    if !rx_str.contains("missing") || !rx_str.contains("error") {
                        cache.insert(tx_hash_bytes, rx.as_bytes()).unwrap();
                    }

                    rx_str
                }
            }
            Err(_) => {
                let rx = rpc.send_request(tx.clone()).await.unwrap();
                let rx_str = rx.as_str().to_string();
                cache.insert(tx_hash_bytes, rx.as_bytes()).unwrap();
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
