use crate::{
    balancer::{
        accept_http::{
            ConnectionParams,
            RequestParams,
        },
        format::get_block_number_from_request,
        processing::{
            can_cache,
            update_rpc_latency,
        },
    },
    database::{
        accept::RequestSender,
        querry_processing::forward_body,
    },
    log_err,
    log_info,
    CacheArgs,
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

use blake3::Hash;
use serde_json::Value;
use simd_json::to_vec;

use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};

/// Forward the request to *a* RPC picked by the algo set by the user.
/// Measures the time needed for a request, and updates the respective
/// RPC lself.
/// In case of a timeout, returns an error.
pub async fn accept_request(
    tx: Request<hyper::body::Incoming>,
    sender: RequestSender,
    connection_params: Arc<ConnectionParams>,
    cache_args: Arc<CacheArgs>,
) {
    // Send request and measure time
    let response: Result<hyper::Response<Full<Bytes>>, Infallible>;
    let rpc_position: Option<usize>;

    // RequestParams from config
    let params = {
        let config_guard = connection_params.config.read().unwrap();
        RequestParams {
            ttl: config_guard.ttl,
            max_retries: config_guard.max_retries,
            header_check: config_guard.header_check,
        }
    };

    // Check if we have the response hashed, and if not forward it
    // to the best available RPC.
    //
    // Also handle cache insertions.
    let time = Instant::now();
    (response, rpc_position) = forward_body(tx, &connection_params, &cache_args, params).await;
    let time = time.elapsed();

    // Send the result
    sender.send(response);

    log_info!("Request time: {:?}", time);

    // `rpc_position` is an Option<> that either contains the index of the RPC
    // we forwarded our request to, or is None if the result was cached.
    //
    // Here, we update the latency of the RPC that was used to process the request
    // if `rpc_position` is Some.
    if let Some(rpc_position) = rpc_position {
        update_rpc_latency(&connection_params.rpc_list, rpc_position, time);
    }
}

/// Check if we should cache the querry, and if so cache it in the DB
pub fn cache_querry(
    method: Value,
    rx: &mut str,
    tx_hash: &Hash,
    cache_args: Arc<CacheArgs>,
    cache: Arc<RwLock<Db>>,
) {
    let tx_string = method.to_string();

    if can_cache(&tx_string, rx) {
        // Insert the response hash into the head_cache
        let num = get_block_number_from_request(method, &cache_args.named_numbers);

        // Insert the key of the request we made into our `head_cache`
        // so we can invalidate it and remove it from the DB if it reorgs.
        if let Some(num) = num {
            if num > *cache_args.finalized_rx.borrow() {
                let mut head_cache = cache_args.head_cache.write().unwrap();
                head_cache.entry(num).or_default().push(tx_hash.to_string());
            }

            // Replace the id with Value::Null and insert the request
            // TODO: kinda cringe how we do this gymnasctics of changing things back and forth
            let mut rx_value: Value = unsafe { simd_json::serde::from_str(rx).unwrap() };
            rx_value["id"] = Value::Null;

            let mut guard = cache.write().unwrap_or_else(|e| {
                // Handle the case where the named_numbers RwLock is poisoned
                log_err!("{}", e);
                e.into_inner()
            });

            guard
                .cache
                .insert(tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice())
                .unwrap();
        }
    }
}
