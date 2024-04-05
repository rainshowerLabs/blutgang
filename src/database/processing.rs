use crate::{
    balancer::{
        format::{
            get_block_number_from_request,
            incoming_to_value,
            replace_block_tags,
        },
        processing::{
            can_cache,
            update_rpc_latency,
        },
        accept_http::{
            ConnectionParams,
            RequestParams,
        },
    },
    database::accept::RequestSender,
    CacheArgs,
    NamedBlocknumbers,
    Rpc,
    log_info,
};

use std::{
    collections::BTreeMap,
    convert::Infallible,
    time::Instant,
    sync::{
        Arc,
        RwLock,
    },
};

use blake3::{
    hash,
    Hash,
};
use serde_json::Value;
use simd_json::to_vec;

use tokio::sync::watch;

use http_body_util::Full;
use hyper::{
    body::Bytes,
    Request,
};

/// Pick RPC and send request to it. In case the result is cached,
/// read and return from the cache.
async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    rpc_list_rwlock: &Arc<RwLock<Vec<Rpc>>>,
    finalized_rx: &watch::Receiver<u64>,
    named_numbers: &Arc<RwLock<NamedBlocknumbers>>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // TODO: do  content type validation more upstream
    // Check if body has application/json
    //
    // Can be toggled via the config. Should be on if we want blutgang to be JSON-RPC compliant.
    // if params.header_check
    //     && tx.headers().get("content-type") != Some(&HeaderValue::from_static("application/json"))
    // {
    //     return (
    //         Ok(hyper::Response::builder()
    //             .status(400)
    //             .body(Full::new(Bytes::from("Improper content-type header")))
    //             .unwrap()),
    //         None,
    //     );
    // }

    // Convert incoming body to serde value
    let mut tx = incoming_to_value(tx).await.unwrap();

    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take().as_u64().unwrap_or(0);

    // Hash the request with either blake3 or xxhash depending on the enabled feature
    let tx_hash;
    #[cfg(not(feature = "xxhash"))]
    {
        tx_hash = hash(tx.to_string().as_bytes());
    }
    #[cfg(feature = "xxhash")]
    {
        tx_hash = xxh3_64(tx.to_string().as_bytes());
    }

    // RPC used to get the response, we use it to update the latency for it later.
    let mut rpc_position;

    // Rewrite named block parameters if possible
    let mut tx = replace_block_tags(&mut tx, named_numbers);

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let rax = get_response!(
        tx,
        cache,
        tx_hash,
        rpc_position,
        id,
        rpc_list_rwlock,
        finalized_rx.clone(),
        named_numbers.clone(),
        head_cache.clone(),
        params.ttl,
        params.max_retries
    );

    // Convert rx to bytes and but it in a Buf
    let body = hyper::body::Bytes::from(rax);

    // Put it in a http_body_util::Full
    let body = Full::new(body);

    // Build the response
    let res = hyper::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap();

    Ok(res)
}

/// Forward the request to *a* RPC picked by the algo set by the user.
/// Measures the time needed for a request, and updates the respective
/// RPC lself.
/// In case of a timeout, returns an error.
pub async fn accept_request(
    mut tx: Request<hyper::body::Incoming>,
    sender: RequestSender,
    cache_args: Arc<CacheArgs>,
    connection_params: ConnectionParams
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
    (response, rpc_position) = forward_body(
        tx,
        &connection_params.rpc_list_rwlock,
        &connection_params.channels.finalized_rx,
        &connection_params.named_numbers,
        &connection_params.head_cache,
    )
    .await;
    let time = time.elapsed();
    log_info!("Request time: {:?}", time);

    // `rpc_position` is an Option<> that either contains the index of the RPC
    // we forwarded our request to, or is None if the result was cached.
    //
    // Here, we update the latency of the RPC that was used to process the request
    // if `rpc_position` is Some.
    if let Some(rpc_position) = rpc_position {
        update_rpc_latency(&connection_params.rpc_list_rwlock, rpc_position, time);
    }

}

/// Check if we should cache the querry, and if so cache it in the DB
pub fn cache_querry(method: Value, rx: &mut str, tx_hash: &Hash, cache_args: Arc<CacheArgs>) {
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

            cache_args
                .cache
                .insert(tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice())
                .unwrap();
        }
    }
}
