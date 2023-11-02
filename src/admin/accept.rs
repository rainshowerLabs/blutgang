use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};
use std::convert::Infallible;
use std::time::Instant;

use std::sync::{
    Arc,
    RwLock,
};

use sled::Db;

use crate::{
    balancer::format::incoming_to_value,
    Rpc,
    Settings,
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
        $id:expr,
        $rpc_list_rwlock:expr,
        //$config:expr
    ) => {{
        // Kinda jank but set the id back to what it was before
        $tx["id"] = $id.into();

        let tx_string = $tx.to_string();

        let mut rx_str = "sdas";

        rx_str
    }};
}

// Execute requesst and construct a HTTP response
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
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
        id,
        rpc_list_rwlock,
        //config,
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
    response = forward_body(tx, &rpc_list_rwlock, cache, config).await;
    let time = time.elapsed();
    println!("\x1b[35mInfo:\x1b[0m Request time: {:?}", time);

    response
}
