use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};
use serde_json::{
    json,
    Value,
    Value::Null,
};

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
};

use jwt::{
    Header,
    Token,
    VerifyWithKey,
};

use sled::Db;

use crate::{
    admin::methods::execute_method,
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
        $poverty_list_rwlock:expr,
        $config:expr,
        $cache:expr,
    ) => {{
        // Execute the request and store it into rx
        let mut rx = match execute_method(
            $tx,
            $rpc_list_rwlock,
            $poverty_list_rwlock,
            Arc::clone(&$config),
            Arc::clone(&$cache),
        ).await {
            Ok(rx) => rx,
            Err(err) => json!({
                "id": Null,
                "jsonrpc": "2.0",
                "result": err.to_string(),
            }),
        };

        // Set the id to whatever it was
        rx["id"] = $id.into();

        let rx_str = rx.to_string();

        rx_str
    }};
}

// Execute requesst and construct a HTTP response
async fn forward_body(
    mut tx: Value,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(tx, id, rpc_list_rwlock, poverty_list_rwlock, config, cache,);

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
    poverty_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
    config: Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;

    let mut tx = incoming_to_value(tx).await.unwrap();

    // If we have JWT enabled check that tx is valid
    //
    // TODO: We are importing 2 random crates and doing this awfulness
    // for JWT support. This makes my eyes bleed and brain hurt. Don't.
    if config.read().unwrap().admin.jwt {
        let token_str = tx["token"].to_string();

        let token: Token<Header, HashMap<String, String>, _> =
            match token_str.verify_with_key(&config.read().unwrap().admin.key) {
                Ok(token) => token,
                Err(err) => {
                    println!("\x1b[31mJWT Auth error:\x1b[0m {}", err);
                    return Ok(hyper::Response::builder()
                        .status(401)
                        .body(Full::new(Bytes::from("Unauthorized")))
                        .unwrap());
                }
            };
        // Reconstruct the TX as a normal json rpc request
        let claims = token.claims();
        tx = json!({
            "id": claims["id"],
            "jsonrpc": "2.0",
            "method": claims["method"],
            "params": claims["params"],
        });
    }

    let time = Instant::now();
    response = forward_body(tx, &rpc_list_rwlock, &poverty_list_rwlock, cache, config).await;
    let time = time.elapsed();
    println!("\x1b[35mInfo:\x1b[0m Request time: {:?}", time);

    response
}
