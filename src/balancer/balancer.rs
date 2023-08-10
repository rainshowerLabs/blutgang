use crate::balancer::format::incoming_to_value;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Request;
use sled::Db;
use std::str::from_utf8;
use std::sync::{
    Arc,
    Mutex,
};

use crate::rpc::types::Rpc;
use std::convert::Infallible;

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
    if tx.as_str().expect("REASON").contains("latest") {
        rax = rpc.send_request(tx).await.unwrap();
    } else {
        let rax = match cache.get(tx.as_str().unwrap().as_bytes()) {
            Ok(rax) => from_utf8(rax.as_ref()).unwrap(),
            Err(_) => {
                let rx = rpc.send_request(tx).await.unwrap();
                cache
                    .insert(
                        tx.clone().as_str().unwrap().as_bytes(),
                        rx.text().await.unwrap().as_bytes(),
                    )
                    .unwrap();

                rx.text().await.unwrap().as_str()
            }
        };
    }

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax.bytes().await.unwrap());

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    //Build the response
    let res = hyper::Response::builder().status(200).body(body).unwrap();

    Ok(res)
}
