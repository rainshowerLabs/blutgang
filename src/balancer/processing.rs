use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        RwLock,
    },
};
use tokio::sync::watch;

use crate::{
    balancer::{
        format::get_block_number_from_request,
        selection::cache_rules::{
            cache_method,
            cache_result,
        },
    },
    health::safe_block::NamedBlocknumbers,
};
use blake3::Hash;
use serde_json::Value;
use simd_json::to_vec;
use sled::Db;

pub fn can_cache(method: &str, result: &str) -> bool {
    if cache_method(method) && cache_result(result) {
        return true;
    }
    false
}

// Check if we should cache the querry, and if so cache it in the DB
pub fn cache_querry(
    rx: &mut String,
    method: &str,
    method_val: Value,
    tx_hash: Hash,
    finalized_rx: &watch::Receiver<u64>,
    named_numbers: &Arc<RwLock<NamedBlocknumbers>>,
    cache: Arc<Db>,
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
) {
    if can_cache(method, rx) {
        // Insert the response hash into the head_cache
        let num = get_block_number_from_request(method_val, named_numbers);

        // Insert the key of the request we made into our `head_cache`
        // so we can invalidate it and remove it from the DB if it reorgs.
        if let Some(num) = num {
            if num > *finalized_rx.borrow() {
                let mut head_cache = head_cache.write().unwrap();
                head_cache
                    .entry(num)
                    .or_insert_with(Vec::new)
                    .push(tx_hash.to_string());
            }

            // Replace the id with Value::Null and insert the request
            let mut rx_value: Value = unsafe { simd_json::serde::from_str(rx).unwrap() };
            rx_value["id"] = Value::Null;

            cache
                .insert(tx_hash.as_bytes(), to_vec(&rx_value).unwrap().as_slice())
                .unwrap();
        }
    }
}
