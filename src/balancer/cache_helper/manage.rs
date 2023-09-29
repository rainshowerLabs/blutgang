use crate::balancer::cache_helper::error::CacheManagerError;
use crate::rpc::types::hex_to_decimal;
use serde_json::to_vec;

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

struct BlocknumIndex<'a> {
    pub method: &'a [u8],
    pub position: usize,
}

// Return the blocknumber from a json-rpc request as a Option<String>, returning None if it cant find anything
pub fn get_block_number_from_request(tx: Value) -> Result<Option<String>, Error> {
    // Return None immediately if params == 0
    if tx["params"].as_array().unwrap().is_empty() {
        return Ok(None);
    }

    // The JSON-RPC standard is all over the place so depending on the method, we need to look at
    // different param indexes. Why? Has i ever???
    let methods = [
        BlocknumIndex {
            method: b"eth_getBalance",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getStorageAt",
            position: 2,
        },
        BlocknumIndex {
            method: b"eth_getTransactionCount",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockTransactionCountByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleCountByBlockNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getCode",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_call",
            position: 1,
        },
        BlocknumIndex {
            method: b"eth_getBlockByNumber",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getTransactionByBlockNumberAndIndex",
            position: 0,
        },
        BlocknumIndex {
            method: b"eth_getUncleByBlockNumberAndIndex",
            position: 0,
        },
    ];

    // Iterate through the array and return the position, return None if not present
    for item in methods.iter() {
        if memmem::find(tx["method"].to_string().as_bytes(), item.method).is_some() {
            let block_number = tx["params"][item.position].to_string().replace('\"', "");

            // If `null` return None
            if block_number == "null" {
                return Ok(None);
            }

            return Ok(Some(block_number));
        }
    }

    Ok(None)
}

// TODO: Consider making get_cache and insert_cache not compatiable with sled cache interface.
// If so, we would be able to return things like blocknumber of the querry which can reduce
// processing time.

// Try to get data from cache, return None if not present
pub fn get_cache(
    tx: Value,
    tx_hash: Hash,
    blocknum_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
) -> Result<Option<IVec>, CacheManagerError> {
    // Extract the blocknumber from the tx
    //
    // If less than blocknum_rx querry cache, if not try head_cache
    let tx_block_number = match get_block_number_from_request(tx.clone()) {
        Ok(block_number) => block_number,
        Err(_) => return Err(CacheManagerError::NumberParseError),
    };
    // `tx_block_number` can be `None`, so just return None immediately if so
    if tx_block_number.is_none() {
        return Ok(None);
    }

    // Parse block number as u64. If the block number is some bullshit like latest, return None
    let tx_block_number = match hex_to_decimal(&tx_block_number.unwrap()) {
        Ok(block_number) => block_number,
        Err(_) => return Ok(None),
    };

    let finalized = *blocknum_rx.borrow();

    if tx_block_number < finalized {
        return Ok(match cache.get(tx_hash.as_bytes()) {
            Ok(value) => value,
            Err(_) => return Err(CacheManagerError::CannotRetrieve),
        });
    }

    let head_cache_guard = head_cache.read().unwrap();

    if let Some(hashmap) = head_cache_guard.get(&tx_block_number) {
        if let Some(value) = hashmap.get(&tx_hash.to_string()) {
            return Ok(Some(value.clone()));
        }
    }

    Ok(None)
}

// Insert data into the appropriate cache
pub fn insert_cache(
    tx: Value,
    tx_hash: Hash,
    blocknum_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
) -> Result<Option<IVec>, CacheManagerError> {
    // Extract the blocknumber from the tx
    //
    // If less than blocknum_rx write to cache, if not head_cache
        let tx_block_number = match get_block_number_from_request(tx.clone()) {
        Ok(block_number) => block_number,
        Err(_) => return Err(CacheManagerError::NumberParseError),
    };

    // Parse block number as u64. If the block number is some bullshit like latest, return None
    let tx_block_number = match hex_to_decimal(&tx_block_number.unwrap()) {
        Ok(block_number) => block_number,
        Err(_) => return Ok(None),
    };

    println!("tx_block_number: {}", tx_block_number);

    let finalized = *blocknum_rx.borrow();

    if tx_block_number < finalized {
        return Ok(cache
            .insert(tx_hash.as_bytes(), to_vec(&tx).unwrap().as_slice())
            .unwrap());
    }

    let mut head_cache_guard = head_cache.write().unwrap();

    if let Some(hashmap) = head_cache_guard.get_mut(&tx_block_number) {
        let a = hashmap.insert(
            tx_hash.to_string(),
            tx.to_string().as_bytes().to_vec().into(),
        );

        return Ok(a);
    }

    let mut hashmap = HashMap::new();
    hashmap.insert(
        tx_hash.to_string(),
        tx.to_string().as_bytes().to_vec().into(),
    );
    head_cache_guard.insert(tx_block_number, hashmap);

    Ok(None)
}
