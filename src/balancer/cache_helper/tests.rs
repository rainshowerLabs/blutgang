use crate::balancer::cache_helper::manage::get_block_number_from_request;
use crate::balancer::cache_helper::manage::get_cache;
use blake3::hash;
use serde_json::json;
use serde_json::to_vec;

use tokio::sync::watch;

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

#[test]
fn get_block_number_from_request_test() {
    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBalance",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getStorageAt",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x0"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1", "0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getTransactionCount",
        "params":["0x407d73d8a49eeb85d32cf465507dd71d507100c1"]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":["latest"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "latest"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":["0x1"]
    });

    assert_eq!(
        get_block_number_from_request(request).unwrap().unwrap(),
        "0x1"
    );

    let request = json!({
        "id":1,
        "jsonrpc":"2.0",
        "method":"eth_getBlockTransactionCountByNumber",
        "params":[]
    });

    assert_eq!(get_block_number_from_request(request).unwrap(), None);
}

// Helper function to create a dummy sled::Db for testing
fn create_dummy_cache() -> Arc<sled::Db> {
    // Replace with your actual sled::Db setup
    Arc::new(sled::Config::new().temporary(true).open().unwrap())
}

// Helper function to create a dummy head cache for testing
fn create_dummy_head_cache() -> Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>> {
    // Replace with your actual head cache setup
    Arc::new(RwLock::new(BTreeMap::<u64, HashMap<String, IVec>>::new()))
}


#[test]
fn test_get_cache_block_less_than_finalized() {
    // Arrange
    let tx = serde_json::json!({"method":"eth_getTransactionByBlockNumberAndIndex","params":["0xa", "0x0"],"id":1,"jsonrpc":"2.0"}); // Replace with your test data
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());
    let blocknum_rx = watch::channel(15).1; // Finalized block number
    let cache = create_dummy_cache();
    let head_cache = create_dummy_head_cache();

    // Act
    let result = get_cache(tx, tx_hash, blocknum_rx, &cache, &head_cache);

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}


#[test]
fn test_get_cache_block_greater_than_finalized_in_cache() {
    // Arrange
    let tx = serde_json::json!({"method":"eth_getTransactionByBlockNumberAndIndex","params":["0xB", "0x0"],"id":1,"jsonrpc":"2.0"}); // Replace with your test data
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());
    let blocknum_rx = watch::channel(10).1; // Finalized block number
    let cache = create_dummy_cache();
    let head_cache = create_dummy_head_cache();

     // Insert test data into head_cache
    let mut hashmap = HashMap::new();
    hashmap.insert(
        tx_hash.to_string(),
        tx.to_string().as_bytes().to_vec().into(),
    );

    {
        head_cache.write().unwrap().insert(11, hashmap);
    }
    assert_eq!(head_cache.read().unwrap().len(), 1);

    // Act
    let result = get_cache(tx.clone(), tx_hash, blocknum_rx, &cache, &head_cache);

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap(), to_vec(&tx).unwrap().as_slice());
}


#[test]
fn test_get_cache_block_greater_than_finalized_in_head_cache() {
    // Arrange
    let tx = serde_json::json!({"method":"eth_getTransactionByBlockNumberAndIndex","params":["0xF", "0x0"],"id":1,"jsonrpc":"2.0"}); // Replace with your test data
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());
    let blocknum_rx = watch::channel(14).1; // Finalized block number
    let cache = create_dummy_cache();
    let head_cache = create_dummy_head_cache();
    let mut hashmap = HashMap::new();

    // Insert test data into the head cache
    {
        let mut head_cache_guard = head_cache.write().unwrap();
        
        hashmap.insert(
            tx_hash.to_string(),
            tx.to_string().as_bytes().to_vec().into(),
        );
        head_cache_guard.insert(15, hashmap);
    }

    // Act
    let result = get_cache(tx.clone(), tx_hash, blocknum_rx, &cache, &head_cache);

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap(), to_vec(&tx).unwrap().as_slice());
}


#[test]
fn test_get_cache_block_number_invalid() {
    // Arrange
    let tx = serde_json::json!({"method":"eth_getTransactionByBlockNumberAndIndex","params":["latest", "0x0"],"id":1,"jsonrpc":"2.0"}); // Replace with your test data
    let tx_hash = hash(to_vec(&tx).unwrap().as_slice());
    let blocknum_rx = watch::channel(15).1; // Finalized block number
    let cache = create_dummy_cache();
    let head_cache = create_dummy_head_cache();

    // Act
    let result = get_cache(tx, tx_hash, blocknum_rx, &cache, &head_cache);

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}
