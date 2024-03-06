use crate::{
    admin::liveready::{
        LiveReadyUpdateSnd,
        accept_health_request,
        accept_readiness_request,
    },
    log_info,
};
use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};

use jsonwebtoken::{
    decode,
    Validation,
};

use serde::{
    Deserialize,
    Serialize,
};

use serde_json::{
    json,
    Value,
    Value::Null,
};

use std::{
    convert::Infallible,
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
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

// For decoding JWT
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    id: Value,
    jsonrpc: Value,
    method: Value,
    params: Value,
    exp: usize,
}

// Macro for getting responses from either the cache or RPC nodes.
//
// Since we don't cache the admin request responses, this functions
// quite differently from the one you'll find in `blutgang/balancer/accept_http.rs`
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

// Execute request and construct a HTTP response
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
    liveness_request_tx: LiveReadyUpdateSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    if tx.uri().path() != "/ready" {
        return accept_readiness_request(liveness_request_tx).await;
    } else if tx.uri().path() != "/healthz" {
        accept_health_request(liveness_request_tx).await;
    }

    let mut tx = incoming_to_value(tx).await.unwrap();

    // If we have JWT enabled check that tx is valid
    if config.read().unwrap().admin.jwt {
        let mut token_str = tx["token"].to_string();
        token_str = token_str.trim_matches('"').to_string();

        let token = match decode::<Claims>(
            &token_str,
            &config.read().unwrap().admin.key,
            &Validation::default(),
        ) {
            Ok(token) => token,
            Err(err) => {
                println!("\x1b[31mJWT Auth error:\x1b[0m {}", err);
                return Ok(hyper::Response::builder()
                    .status(401)
                    .body(Full::new(Bytes::from("Unauthorized or invalid token")))
                    .unwrap());
            }
        };

        // Reconstruct the TX as a normal json rpc request
        log_info!("JWT claims: {:?}", token);

        tx = json!({
            "id": token.claims.id,
            "jsonrpc": "2.0",
            "method": token.claims.method,
            "params": token.claims.params,
        });
    }

    // Send the request off to be processed
    let time = Instant::now();
    let response = forward_body(tx, &rpc_list_rwlock, &poverty_list_rwlock, cache, config).await;
    let time = time.elapsed();
    log_info!("Request time: {:?}", time);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::DecodingKey;

    // Helper function to create a test Settings config
    fn create_test_settings() -> Arc<RwLock<Settings>> {
        let mut config = Settings::default();
        config.do_clear = true;
        config.admin.key = DecodingKey::from_secret(b"some-key");
        Arc::new(RwLock::new(config))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> Arc<Db> {
        let db = sled::Config::new().temporary(true);
        let db = db.open().unwrap();

        Arc::new(db)
    }

    #[tokio::test]
    async fn test_forward_body() {
        let settings = create_test_settings();
        let cache = create_test_cache();
        let rpc_list = Arc::new(RwLock::new(vec![]));
        let poverty_list = Arc::new(RwLock::new(vec![]));

        // Create a test request (use the actual request format here)
        let tx = json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "blutgang_ttl",
            "params": [],
        });

        // Call forward_body with the test data
        let result = forward_body(
            tx.clone(),
            &rpc_list,
            &poverty_list,
            cache.clone(),
            settings,
        )
        .await;

        // You can assert that the result matches the expected outcome
        assert!(result.is_ok());

        // Additional assertions can be added based on expected behavior
    }
}
