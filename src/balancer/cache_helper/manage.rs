use crate::rpc::types::hex_to_decimal;

use blake3::Hash;
use memchr::memmem;
use serde_json::{
    Error,
    Value,
};
use sled::IVec;

use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    sync::{
        Arc,
        RwLock,
    },
};

struct RequestPos<'a> {
    pub method: &'a [u8],
    pub position: Option<usize>,
}

// Return the blocknumber from a json-rpc request as a Option<String>, returning None if it cant find anything
pub fn get_block_number_from_request(tx: Value) -> Result<Option<String>, Error> {
    // Return None immediately if params == 0
    if tx["params"].as_array().unwrap().len() == 0 {
        return Ok(None);
    }

    // The JSON-RPC standard is all over the place so depending on the method, we need to look at
    // different param indexes. Why? Has i ever???
    let methods = [
        RequestPos {
            method: b"eth_getBalance",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getStorageAt",
            position: Some(2),
        },
        RequestPos {
            method: b"eth_getTransactionCount",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getBlockTransactionCountByNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getUncleCountByBlockNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getCode",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_call",
            position: Some(1),
        },
        RequestPos {
            method: b"eth_getBlockByNumber",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getTransactionByBlockNumberAndIndex",
            position: Some(0),
        },
        RequestPos {
            method: b"eth_getUncleByBlockNumberAndIndex",
            position: Some(0),
        },
    ];

    // Iterate through the array and return the position, return None if not present
    for item in methods.iter() {
        if memmem::find(tx["method"].to_string().as_bytes(), item.method).is_some() {
            let pos = item.position.unwrap();
            let block_number = tx["params"][pos].to_string().replace("\"", "");

            // If `null` return None
            if block_number == "null" {
                return Ok(None);
            }

            return Ok(Some(block_number));
        }
    }

    Ok(None)
}

// Try to get data from cache, return None if not present
pub fn get_cache(
    tx: Value,
    tx_hash: Hash,
    blocknum_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
) -> Result<Option<IVec>, Box<dyn std::error::Error>> {
    // Extract the blocknumber from the tx
    //
    // If less than blocknum_rx querry cache, if not try head_cache
    let tx_block_number = get_block_number_from_request(tx.clone())?;
    // `tx_block_number` can be `None`, so just return None immediately if so
    if tx_block_number.is_none() {
        return Ok(None);
    }

    // Parse block number as u64. If the block number is some bullshit like latest, return None
    let tx_block_number = match hex_to_decimal(&tx_block_number.unwrap()) {
        Ok(block_number) => block_number,
        Err(_) => return Ok(None),
    };

    let finalized = blocknum_rx.borrow().clone();

    if tx_block_number < finalized {
        return Ok(cache.get(tx_hash.as_bytes())?);
    }

    let head_cache_guard = head_cache.read().unwrap();

    if let Some(hashmap) = head_cache_guard.get(&tx_block_number) {
        if let Some(value) = hashmap.get(&tx_hash.to_string()) {
            return Ok(Some(value.clone()));
        }
    }

    Ok(None)
}
