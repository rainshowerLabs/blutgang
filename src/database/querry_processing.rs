use std::convert::Infallible;

use crate::{
    balancer::{
        accept_http::{
            ConnectionParams,
            RequestParams,
        },
        format::{
            incoming_to_value,
            replace_block_tags,
        },
        processing::CacheArgs,
        selection::select::pick,
    },
    cache_error,
    database::error::DbError,
    log_info,
    log_wrn,
    no_rpc_available,
    print_cache_error,
};

use serde_json::Value;

use tokio::time::{
    timeout,
    Duration,
};

use blake3::hash;

use http_body_util::Full;
use hyper::{
    header::HeaderValue,
    body::Bytes,
    Request,
};

/// Pick RPC and send request to it. In case the result is cached,
/// read and return from the cache.
pub async fn forward_body(
    tx: Request<hyper::body::Incoming>,
    con_params: &ConnectionParams,
    cache_args: &CacheArgs,
    params: RequestParams,
) -> (
    Result<hyper::Response<Full<Bytes>>, Infallible>,
    Option<usize>,
) {
    // TODO: do content type validation more upstream
    // Check if body has application/json
    //
    // Can be toggled via the config. Should be on if we want blutgang to be JSON-RPC compliant.
    if params.header_check
        && tx.headers().get("content-type") != Some(&HeaderValue::from_static("application/json"))
    {
        return (
            Ok(hyper::Response::builder()
                .status(400)
                .body(Full::new(Bytes::from("Improper content-type header")))
                .unwrap()),
            None,
        );
    }

    // Convert incoming body to serde value
    let tx = incoming_to_value(tx).await.unwrap();

    // Get the response from either the DB or from a RPC. If it timeouts, retry.
    let (rax, position) = get_response(tx, con_params, cache_args, params)
        .await
        .unwrap();

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

    (Ok(res), position)
}

async fn get_response(
    tx: Value,
    con_params: &ConnectionParams,
    cache_args: &CacheArgs,
    params: RequestParams,
) -> Result<(String, Option<usize>), DbError> {
    // Get the id of the request and set it to 0 for caching
    //
    // We're doing this ID gymnastics because we're hashing the
    // whole request and we don't want the ID as it's arbitrary
    // and does not impact the request result.
    let id = tx["id"].take();

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
    // TODO: we are NOT doing this here!
    let mut rpc_position;

    // Rewrite named block parameters if possible
    let mut tx = replace_block_tags(&mut tx, &cache_args.named_numbers);

    match cache_args.cache.get(tx_hash.as_bytes()) {
        Ok(Some(mut rax)) => {
            rpc_position = None;
            // Reconstruct ID
            let mut cached: Value = simd_json::serde::from_slice(&mut rax).unwrap();

            cached["id"] = id;
            return Ok((cached.to_string(), rpc_position));
        }
        Ok(None) => fetch_from_rpc(tx, id, con_params, cache_args, params).await,
        Err(_) => {
            // If anything errors send an rpc request and see if it works, if not then gg
            print_cache_error!();
            rpc_position = None;
            return Ok((cache_error!(), rpc_position));
        }
    }
}

async fn fetch_from_rpc(
    tx: Value,
    id: Value,
    con_params: &ConnectionParams,
    cache_args: &CacheArgs,
    params: RequestParams,
) -> Result<(String, Option<usize>), DbError> {
    // Kinda jank but set the id back to what it was before
    tx["id"] = id;
    let ttl = params.ttl;
    let max_retries = params.max_retries;

    // Loop until we get a response
    let mut rx;
    let mut retries = 0;
    loop {
        // Get the next Rpc in line.
        // TODO: can we do something about this being blocking?
        let mut rpc;
        let rpc_position;
        {
            let mut rpc_list = con_params.rpc_list.write().unwrap();
            (rpc, rpc_position) = pick(&mut rpc_list);
        }
        log_info!("Forwarding to: {}", rpc.name);

        // Check if we have any RPCs in the list, if not return error
        if rpc_position == None {
            return (no_rpc_available!(), None);
        }

        // Send the request. And return a timeout if it takes too long
        //
        // Check if it contains any errors or if its `latest` and insert it if it isn't
        match timeout(
            Duration::from_millis(ttl.try_into().unwrap()),
            rpc.send_request(tx.clone()),
        )
        .await
        {
            Ok(rxa) => {
                rx = rxa.unwrap();
                break;
            }
            Err(_) => {
                log_wrn!("\x1b[93mWrn:\x1b[0m An RPC request has timed out, picking new RPC and retrying.");
                rpc.update_latency(ttl as f64);
                retries += 1;
            }
        };

        if retries == max_retries {
            return (timed_out!(), rpc_position);
        }
    }
}
