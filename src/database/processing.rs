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
