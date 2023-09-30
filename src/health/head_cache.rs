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
    head_cache: &Arc<RwLock<BTreeMap<u64, String>>>,
    mut finalized_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Loop for waiting on new values from the finalized_rx channel
    while finalized_rx.changed().await.is_ok() {
        let new_finalized = *finalized_rx.borrow();

        println!(
            "New finalized block {}, flushing head cache to disk...",
            new_finalized
        );

        let _ = flush_cache(head_cache, new_finalized, cache);
    }

    Ok(())
}

// Flushes all cache to disk until and including the blocknumber provided
fn flush_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, String>>>,
    block_number: u64,
    cache: &Arc<sled::Db>,
) -> Result<(), Box<dyn std::error::Error>> {
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