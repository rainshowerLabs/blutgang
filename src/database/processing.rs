use crate::{
    CacheArgs,
    balancer::{
        format::get_block_number_from_request,
        processing::can_cache,
    },
};

use simd_json::to_vec;
use std::sync::Arc;
use serde_json::Value;
use blake3::Hash;

/// Forward the request to *a* RPC picked by the algo set by the user.
/// Measures the time needed for a request, and updates the respective
/// RPC lself.
/// In case of a timeout, returns an error.
pub async fn accept_request(
    mut tx: Request<hyper::body::Incoming>,
    connection_params: ConnectionParams,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // Check if the request is a websocket upgrade request.
    if is_upgrade_request(&tx) {
        log_info!("Received WS upgrade request");

        if !connection_params.config.read().unwrap().is_ws {
            return rpc_response!(
                500,
                Full::new(Bytes::from(
                    "{code:-32005, message:\"error: WebSockets are disabled!\"}".to_string(),
                ))
            );
        }

        let (response, websocket) = match upgrade(&mut tx, None) {
            Ok((response, websocket)) => (response, websocket),
            Err(e) => {
                log_err!("Websocket upgrade error: {}", e);
                return rpc_response!(500, Full::new(Bytes::from(
                    "{code:-32004, message:\"error: Websocket upgrade error! Try again later...\"}"
                        .to_string(),
                )));
            }
        };

        let cache_args = CacheArgs {
            finalized_rx: connection_params.channels.finalized_rx.as_ref().clone(),
            named_numbers: connection_params.named_numbers.clone(),
            cache: connection_params.cache,
            head_cache: connection_params.head_cache.clone(),
        };

        // Spawn a task to handle the websocket connection.
        tokio::task::spawn(async move {
            if let Err(e) = serve_websocket(
                websocket,
                connection_params.channels.incoming_tx,
                connection_params.channels.outgoing_rx,
                connection_params.sub_data.clone(),
                cache_args,
            )
            .await
            {
                log_err!("Websocket connection error: {}", e);
            }
        });

        // Return the response so the spawned future can continue.
        return Ok(response);
    }

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
        connection_params.cache,
        params,
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

    response
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
