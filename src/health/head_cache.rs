use std::{
    collections::{
        BTreeMap,
    },
    sync::{
        Arc,
        RwLock,
    },
};

use sled::{
    Batch,
};

pub async fn manage_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    mut blocknum_rx: tokio::sync::watch::Receiver<u64>,
    mut finalized_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
) -> Result<(), sled::Error> {
    let mut block_number = 0;
    // Loop for waiting on new values from the finalized_rx channel
    while blocknum_rx.changed().await.is_ok() {
        let new_block = *blocknum_rx.borrow();

        // If a new block is less or equal(todo) to the last block in our cache,
        // that means that the chain has experienced a reorg and that we should
        // remove everything from the last block to the `new_block`
        if new_block <= block_number {
            handle_reorg(&head_cache, block_number, new_block, &cache)?;
        }

        if finalized_rx.changed().await.is_ok() {
            let _ = remove_stale(&head_cache, *finalized_rx.borrow(), &cache);
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
    for i in block_number..new_block {
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
    cache: &Arc<sled::Db>,
) -> Result<(), sled::Error> {
    // sled batch
    let mut _batch = Batch::default();

    // Get the lowest block_number from the BTreeMap
    let mut head_cache_guard = head_cache.write().unwrap();

    let oldest = match head_cache_guard.iter().next() {
        Some((oldest, _)) => *oldest,
        None => return Ok(()), // Return early if the map is empty
    };

    // Remove all entries from the head_cache up to block_number
    for i in oldest..=block_number {
        head_cache_guard.remove(&i);
    }

    // Apply the batch to the cache
    cache.apply_batch(_batch)?;

    Ok(())
}