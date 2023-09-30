use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        RwLock,
    },
};

use sled::Batch;
use tokio_stream::{
    wrappers::WatchStream,
    StreamExt,
};

pub async fn manage_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    blocknum_rx: tokio::sync::watch::Receiver<u64>,
    finalized_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
) -> Result<(), sled::Error> {
    let mut block_number = 0;
    let mut last_finalized = 0;

    let mut blocknum_stream = WatchStream::new(blocknum_rx.clone());

    // Loop for waiting on new values from the finalized_rx channel
    while blocknum_stream.next().await.is_some() {
        let new_block = *blocknum_rx.borrow();

        // If a new block is less or equal(todo) to the last block in our cache,
        // that means that the chain has experienced a reorg and that we should
        // remove everything from the last block to the `new_block`
        if new_block <= block_number {
            println!("Reorg detected!\nRemoving stale entries from the cache...");
            handle_reorg(head_cache, block_number, new_block, cache)?;
        }

        // Check if finalized_stream has changed
        if last_finalized != *finalized_rx.borrow() {
            last_finalized = *finalized_rx.borrow();
            println!("New finalized block!\nRemoving stale entries from the cache...");
            // Remove stale entries from the head_cache
            remove_stale(head_cache, last_finalized)?;
        }

        block_number = new_block;
    }

    Ok(())
}

// We use the head_cache to store keys of querries we made near the tip
// If a reorg happens, we need to remove all querries in the reorg range
// from the sled database.
fn handle_reorg(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    block_number: u64,
    new_block: u64,
    cache: &Arc<sled::Db>,
) -> Result<(), sled::Error> {
    // sled batch
    let mut batch = Batch::default();

    // Go over the head cache and get all the keys from block_number to new_block
    let mut head_cache_guard = head_cache.write().unwrap();
    for i in block_number..new_block + 1 {
        if let Some(keys) = head_cache_guard.get(&i) {
            for key in keys {
                batch.remove(key.as_bytes());
            }
            // Remove the entry from the head_cache
            head_cache_guard.remove(&i);
        }
    }

    // Apply the batch to the cache
    cache.apply_batch(batch)?;

    Ok(())
}

// Removes stale entries from `head_cache`
fn remove_stale(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    block_number: u64,
) -> Result<(), sled::Error> {
    // Get the lowest block_number from the BTreeMap
    let mut head_cache_guard = head_cache.write().unwrap();

    let oldest = match head_cache_guard.iter().next() {
        Some((oldest, _)) => *oldest,
        None => return Ok(()), // Return early if the map is empty
    };

    // Remove all entries from the head_cache up to block_number
    for i in oldest..=block_number + 1 {
        head_cache_guard.remove(&i);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sled::Config;

    // #[tokio::test]
    // async fn test_manage_cache() {
    //     // Create test data and resources
    //     let head_cache = Arc::new(RwLock::new(BTreeMap::new()));
    //     let (blocknum_tx, blocknum_rx) = tokio::sync::watch::channel(0);
    //     let (finalized_tx, finalized_rx) = tokio::sync::watch::channel(0);
    //     let cache = Arc::new(Config::new().temporary(true).open().unwrap());

    //     // Spawn the manage_cache function in a separate thread
    //     let manage_cache_handle = tokio::spawn(async move {
    //         let head_cache_clone = Arc::clone(&head_cache);
    //         let cache_clone = Arc::clone(&cache);

    //         let _ = manage_cache(&head_cache_clone, blocknum_rx, finalized_rx, &cache_clone).await;
    //     });

    //     // Simulate changes in blocknum_rx and finalized_rx
    //     blocknum_tx.send(5).unwrap();
    //     finalized_tx.send(2).unwrap();

    //     // Wait for the manage_cache function to complete
    //     let result = manage_cache_handle.await;
    //     assert!(result.is_ok());
    // }

    #[test]
    fn test_handle_reorg() {
        // Create test data and resources
        let head_cache = Arc::new(RwLock::new(BTreeMap::new()));
        let cache = Arc::new(Config::new().temporary(true).open().unwrap());

        let _ = cache.insert("key1", "value1");
        let _ = cache.insert("key2", "value2");
        let _ = cache.insert("key3", "value3");

        // Add some data to the head_cache
        {
            let mut head_cache_guard = head_cache.write().unwrap();
            head_cache_guard.insert(1, vec!["key1".to_string()]);
            head_cache_guard.insert(2, vec!["key2".to_string()]);
            head_cache_guard.insert(3, vec!["key3".to_string()]);
        }

        // Call handle_reorg
        let result = handle_reorg(&head_cache, 2, 3, &cache);

        // Verify the result and check if the data is removed from the cache
        assert!(result.is_ok());
        let head_cache_guard = head_cache.read().unwrap();
        assert!(head_cache_guard.contains_key(&1));
        assert!(!head_cache_guard.contains_key(&2));
        assert!(!head_cache_guard.contains_key(&3));

        // Check if the data is removed from the cache
        let key1 = cache.get("key1").unwrap();
        assert!(key1.is_some());
        let key2 = cache.get("key2").unwrap();
        assert!(key2.is_none());
        let key3 = cache.get("key3").unwrap();
        assert!(key3.is_none());
    }

    #[test]
    fn test_remove_stale() {
        // Create test data and resources
        let head_cache = Arc::new(RwLock::new(BTreeMap::new()));

        // Add some data to the head_cache
        {
            let mut head_cache_guard = head_cache.write().unwrap();
            head_cache_guard.insert(1, vec!["key1".to_string()]);
            head_cache_guard.insert(2, vec!["key2".to_string()]);
        }

        // Call remove_stale
        let result = remove_stale(&head_cache, 1);

        // Verify the result and check if the data is removed from the cache
        assert!(result.is_ok());
        let head_cache_guard = head_cache.read().unwrap();
        assert!(!head_cache_guard.contains_key(&1));
        assert!(!head_cache_guard.contains_key(&2));
    }
}
