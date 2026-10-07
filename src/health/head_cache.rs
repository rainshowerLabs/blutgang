use crate::database::{
    accept::db_batch,
    error::DbError,
    types::{
        Batch,
        GenericBytes,
        RequestBus,
    },
};

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        RwLock,
    },
};

use tokio_stream::{
    wrappers::WatchStream,
    StreamExt,
};

/// Check if we need to do a reorg or if a new block has finalized.
pub async fn manage_cache<K, V>(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<K>>>>,
    blocknum_rx: tokio::sync::watch::Receiver<u64>,
    finalized_rx: Arc<tokio::sync::watch::Receiver<u64>>,
    cache: RequestBus<K, V>,
) -> Result<(), DbError>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let mut block_number = 0;
    let mut last_finalized = 0;

    let mut blocknum_stream = WatchStream::new(blocknum_rx.clone());

    // Loop for waiting on new values from the finalized_rx channel
    while blocknum_stream.next().await.is_some() {
        let new_block = *blocknum_rx.borrow();

        // If a new block is less or equal to the last block in our cache,
        // that means that the chain has experienced a reorg and that we should
        // remove everything from `new_block` through the previous head.
        if new_block <= block_number {
            tracing::warn!("Reorg detected! Removing stale entries from the cache.");
            handle_reorg(head_cache, block_number, new_block, cache.clone()).await?;
        }

        // Check if finalized_stream has changed
        if last_finalized != *finalized_rx.borrow() {
            last_finalized = *finalized_rx.borrow();
            tracing::info!("New finalized block! Removing stale entries from the cache.");
            // Remove stale entries from the head_cache
            remove_stale(head_cache, last_finalized)?;
        }

        block_number = new_block;
    }
    Ok(())
}

/// We use the head_cache to store keys of querries we made near the tip
/// If a reorg happens, we need to remove all queries in the reorg range
/// from the sled database.
async fn handle_reorg<K, V>(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<K>>>>,
    block_number: u64,
    new_block: u64,
    cache: RequestBus<K, V>,
) -> Result<(), DbError>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let range = new_block..=block_number;
    let mut batch = Batch::with_capacity(range.clone().count());

    // Include both the replacement head and every block rolled back above it.
    {
        let mut head_cache_guard = head_cache.write().unwrap();
        for i in range {
            if let Some(keys) = head_cache_guard.get(&i).cloned() {
                for key in keys {
                    batch.delete(key);
                }
                // Remove the entry from the head_cache
                head_cache_guard.remove(&i);
            }
        }
    }

    // Send the batch to the cache
    drop(db_batch(&cache, batch).await);

    Ok(())
}

/// Removes stale entries from `head_cache`
///
/// Once a new block finalizes, we can be sure that certain TXs wont
/// reorg, so theyre safe to be permanantly in the cache.
fn remove_stale<K: GenericBytes>(
    head_cache: &Arc<RwLock<BTreeMap<u64, Vec<K>>>>,
    block_number: u64,
) -> Result<(), DbError> {
    // Get the lowest block_number from the BTreeMap
    let mut head_cache_guard = head_cache.write().unwrap();

    let oldest = match head_cache_guard.iter().next() {
        Some((oldest, _)) => *oldest,
        None => return Ok(()), // Return early if the map is empty
    };

    // Only finalized blocks can be forgotten; the next block can still reorg.
    for i in oldest..=block_number {
        head_cache_guard.remove(&i);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::types::DbRequest;
    use crate::database_processing;
    use crate::db_get;
    use sled::{
        Config,
        Db,
    };
    use tokio::sync::mpsc;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handle_reorg_backwards() {
        // Create test data and resources
        let head_cache = Arc::new(RwLock::new(BTreeMap::new()));
        let cache = Config::tmp().unwrap();
        let cache = Db::open_with_config(&cache).unwrap();

        let _ = cache.insert("key1", "value1");
        let _ = cache.insert("key2", "value2");
        let _ = cache.insert("key3", "value3");

        // Add some data to the head_cache
        {
            let mut head_cache_guard = head_cache.write().unwrap();
            head_cache_guard.insert(1, vec!["key1".as_bytes()]);
            head_cache_guard.insert(2, vec!["key2".as_bytes()]);
            head_cache_guard.insert(3, vec!["key3".as_bytes()]);
        }

        let (db_tx, db_rx) = mpsc::unbounded_channel::<DbRequest<&[u8], &[u8]>>();
        tokio::task::spawn(database_processing(db_rx, cache));

        // Match manage_cache's argument order when the head rolls back from 3 to 2.
        let result = handle_reorg(&head_cache, 3, 2, db_tx.clone()).await;

        // Verify the result and check if the data is removed from the cache
        assert!(result.is_ok(), "handle_reorg failed");
        let head_cache_guard = head_cache.read().expect("failed to read head cache");
        assert!(
            head_cache_guard.contains_key(&1),
            "head cache does not contain key1"
        );
        assert!(
            !head_cache_guard.contains_key(&2),
            "head cache should not contain key2"
        );
        assert!(
            !head_cache_guard.contains_key(&3),
            "head cache should not contain key3"
        );

        // Check if the data is removed from the cache
        let key1 = db_get!(db_tx.clone(), "key1".as_bytes()).unwrap();
        assert!(key1.is_some(), "failed to get key1 from db");
        let key2 = db_get!(db_tx.clone(), "key2".as_bytes()).unwrap();
        assert!(
            key2.is_none(),
            "successfully got key2 from db which should have failed"
        );
        let key3 = db_get!(db_tx.clone(), "key3".as_bytes()).unwrap();
        assert!(
            key3.is_none(),
            "successfully got key3 from db which should have failed"
        );
    }

    async fn assert_same_height_reorg(prune_finalized: bool) {
        let head_cache = Arc::new(RwLock::new(BTreeMap::from([
            (1, vec![b"earlier".as_slice()]),
            (2, vec![b"head_a".as_slice(), b"head_b".as_slice()]),
        ])));
        let config = Config::tmp().unwrap();
        let cache = Db::open_with_config(&config).unwrap();
        for key in [b"earlier".as_slice(), b"head_a", b"head_b"] {
            cache.insert(key, b"cached response").unwrap();
        }

        let (db_tx, db_rx) = mpsc::unbounded_channel::<DbRequest<&[u8], &[u8]>>();
        tokio::task::spawn(database_processing(db_rx, cache));

        if prune_finalized {
            remove_stale(&head_cache, 1).unwrap();
        }
        handle_reorg(&head_cache, 2, 2, db_tx.clone())
            .await
            .unwrap();

        // Finalized/earlier responses stay cached, but every response for the
        // replaced head must be evicted, even just after its parent finalizes.
        assert!(db_get!(db_tx.clone(), b"earlier".as_slice())
            .unwrap()
            .is_some());
        for key in [b"head_a".as_slice(), b"head_b"] {
            assert!(
                db_get!(db_tx.clone(), key).unwrap().is_none(),
                "cached response for the replaced head survived invalidation"
            );
        }
        assert!(!head_cache.read().unwrap().contains_key(&2));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handle_reorg_same_height() {
        assert_same_height_reorg(false).await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_handle_reorg_after_parent_finalizes() {
        assert_same_height_reorg(true).await;
    }

    #[test]
    fn test_remove_stale_keeps_unfinalized_blocks() {
        // Create test data and resources
        let head_cache = Arc::new(RwLock::new(BTreeMap::new()));

        // Add some data to the head_cache
        {
            let mut head_cache_guard = head_cache.write().unwrap();
            head_cache_guard.insert(1, vec!["key1".as_bytes()]);
            head_cache_guard.insert(2, vec!["key2".as_bytes()]);
        }

        // Call remove_stale
        let result = remove_stale(&head_cache, 1);

        // Verify the result and check if the data is removed from the cache
        assert!(result.is_ok());
        let head_cache_guard = head_cache.read().unwrap();
        assert!(!head_cache_guard.contains_key(&1));
        assert!(head_cache_guard.contains_key(&2));
    }

    #[test]
    fn test_remove_stale_max_block_number() {
        let head_cache = Arc::new(RwLock::new(BTreeMap::from([
            (u64::MAX - 1, vec![b"parent".as_slice()]),
            (u64::MAX, vec![b"head".as_slice()]),
        ])));

        remove_stale(&head_cache, u64::MAX).unwrap();

        assert!(head_cache.read().unwrap().is_empty());
    }
}
