use std::convert::Infallible;
use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};

use std::sync::{
    Arc,
    RwLock,
};

use sled::Db;

use crate::{
    Settings,
    Rpc,
};

// use tokio::net::TcpListener;
// use hyper::{
//     server::conn::http1,
//     service::service_fn,
// };
// use hyper_util_blutgang::rt::TokioIo;

// Macro for getting responses from either the cache or RPC nodes.
//
// Since we don't cache the admin request responses, this functions
// quite differently from the one you'll find in `blutgang/balancer/balancer.rs`
macro_rules! get_response {
    (
        $tx:expr,
        $cache:expr,
        $id:expr,
        $rpc_list_rwlock:expr,
        $config:expr
    ) => {
            // Kinda jank but set the id back to what it was before
            $tx["id"] = $id.into();

            let tx_string = $tx.to_string();

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

                // TODO: dont hardcode this
                if retries == 32 {
                    return (timed_out!(), $rpc_position,);
                }
            }

            let mut rx_str = rx.as_str().to_string();

            // Don't cache responses that contain errors or missing trie nodes
            if cache_method(&tx_string) && cache_result(&rx) {
                // Insert the response hash into the head_cache
                let num = get_block_number_from_request($tx, $named_numbers);
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
                    let mut rx_value: Value = unsafe {
                        simd_json::serde::from_str(&mut rx_str).unwrap()
                    };
                    rx_value["id"] = Value::Null;

                    $cache.insert($tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice()).unwrap();
                }

            }

            rx_str
        }
            
}

// Execute requesst and construct a HTTP response
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
) {
    // Convert incoming body to serde value
    let mut tx = incoming_to_value(tx).await.unwrap();

    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache,
        id,
        rpc_list_rwlock,
        config,
    );

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    //Build the response
    let res = hyper::Response::builder().status(200).body(body).unwrap();

    Ok(res)
}

// Accept admin request, self explanatory
pub async fn accept_admin_request(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
	let response: Result<hyper::Response<Full<Bytes>>, Infallible>;

    let time = Instant::now();
    response = forward_body(
        tx,
        &rpc_list_rwlock,
        cache,
        config,
    )
    .await;
    let time = time.elapsed();
    println!("\x1b[35mInfo:\x1b[0m Request time: {:?}", time);

    Ok(response)
}
