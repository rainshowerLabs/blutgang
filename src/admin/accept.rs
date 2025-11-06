use crate::{
    admin::liveready::{
        accept_health_request,
        accept_readiness_request,
        LiveReadyRequestSnd,
    },
    database::types::{
        GenericBytes,
        RequestBus,
    },
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

use crate::{
    admin::methods::execute_method,
    balancer::format::incoming_to_value,
    Rpc,
    Settings,
};

/// For decoding JWT
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    id: Value,
    jsonrpc: Value,
    method: Value,
    params: Value,
    exp: usize,
}

/// Macro for getting responses from either the cache or RPC nodes.
///
/// Since we don't cache the admin request responses, this functions
/// quite differently from the one you'll find in `blutgang/balancer/accept_http.rs`
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
            $cache.clone(),
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

/// Execute request and construct a HTTP response
async fn forward_body<K, V>(
    mut tx: Value,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    cache: RequestBus<K, V>,
    config: Arc<RwLock<Settings>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible>
where
    K: GenericBytes,
    V: GenericBytes,
{
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

/// Accept admin request, self explanatory
pub async fn accept_admin_request<K, V>(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    poverty_list_rwlock: Arc<RwLock<Vec<Rpc>>>,
    cache: RequestBus<K, V>,
    config: Arc<RwLock<Settings>>,
    liveness_request_tx: LiveReadyRequestSnd,
) -> Result<hyper::Response<Full<Bytes>>, Infallible>
where
    K: GenericBytes,
    V: GenericBytes,
{
    if tx.uri().path() == "/ready" {
        return accept_readiness_request(liveness_request_tx).await;
    } else if tx.uri().path() == "/health" {
        return accept_health_request(liveness_request_tx).await;
    }

    let mut tx = match incoming_to_value(tx).await {
        Ok(res) => res,
        Err(err) => {
            tracing::error!(?err, "Admin request malformed");
            return Ok(hyper::Response::builder()
                .status(401)
                .body(Full::new(Bytes::from("Invalid admin request format")))
                .unwrap());
        }
    };

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
                tracing::error!(?err, "JWT Auth error");
                return Ok(hyper::Response::builder()
                    .status(401)
                    .body(Full::new(Bytes::from("Unauthorized or invalid token")))
                    .unwrap());
            }
        };

        // Reconstruct the TX as a normal json rpc request
        tracing::info!(?token, "JWT claims");

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
    tracing::info!(?time, "Request time");

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database_processing;
    use jsonwebtoken::DecodingKey;
    use sled::Config;
    use sled::Db;
    use tokio::sync::mpsc;

    // Helper function to create a test Settings config
    fn create_test_settings() -> Arc<RwLock<Settings>> {
        let mut config = Settings::default();
        config.do_clear = true;
        config.admin.key = DecodingKey::from_secret(b"some-key");
        Arc::new(RwLock::new(config))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> RequestBus<Vec<u8>, Vec<u8>> {
        let cache = Config::tmp().unwrap();
        let cache = Db::open_with_config(&cache).unwrap();
        let (db_tx, db_rx) = mpsc::unbounded_channel();
        tokio::task::spawn(database_processing(db_rx, cache));

        db_tx
    }

    #[tokio::test]
    #[serial_test::serial]
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
