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

use sled::{ IVec, Batch };

pub async fn manage_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
    mut blocknum_rx: tokio::sync::watch::Receiver<u64>,
    cache: &Arc<sled::Db>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Loop for waiting on new values from the blocknum_rx channel
    while blocknum_rx.changed().await.is_ok() {
        let new_finalized = blocknum_rx.borrow().clone();
        println!("received: {:?}", new_finalized);
        let _ = flush_cache(head_cache, new_finalized, cache);
    }

    Ok(())
}

// Flushes all cache to disk until and including the blocknumber provided
fn flush_cache(
    head_cache: &Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>>,
    block_number: u64,
    cache: &Arc<sled::Db>,
) -> Result<(), Box<dyn std::error::Error>>{
    // sled batch
    let mut batch = Batch::default();

    // Get the lowest block_number from the BTreeMap
    let mut head_cache_guard = head_cache.write().unwrap();

    let oldest = match head_cache_guard.iter().next() {
        Some((oldest, _)) => *oldest,
        None => return Ok(()), // Return early if the map is empty
    };

    // Iterate from `oldest` to `block_number` and cache all queries into `cache`
    for block in oldest..=block_number+1 {
        if let Some(data) = head_cache_guard.get(&block) {
            for (key, value) in data.iter() {
                // Insert data into the sled batch
                batch.insert(key.as_bytes(), value.clone());
            }
            // Remove the last element
            head_cache_guard.pop_last();
        }
    }

    // Apply the batch to the cache
    cache.apply_batch(batch)?;

    Ok(())
}

mod tests {
    use super::*;
    use sled::Config;

    #[test]
    fn test_flush_cache() -> Result<(), Box<dyn std::error::Error>> {
        // Create a temporary directory for the sled database
        let db_config = Config::default()
            .temporary(true)
            .flush_every_ms(None);

        // Open a sled database
        let cache = Arc::new(db_config.open()?);

        // Create a head_cache with some sample data
        let head_cache: Arc<RwLock<BTreeMap<u64, HashMap<String, IVec>>>> = Arc::new(RwLock::new(
            vec![
                (
                    1, // Block number
                    [
                        ("key1".to_string(), IVec::from("value1")),
                        ("key2".to_string(), IVec::from("value2")),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                ),
                (
                    2, // Block number
                    [("key3".to_string(), IVec::from("value3"))]
                        .iter()
                        .cloned()
                        .collect(),
                ),
                (
                    3, // Block number
                    [("key4".to_string(), IVec::from("value3"))]
                        .iter()
                        .cloned()
                        .collect(),
                ),
            ]
            .into_iter()
            .collect(),
        ));

        // Call flush_cache to flush data from block 1 to 2
        flush_cache(&head_cache, 2, &cache)?;

        // Check if the data was correctly flushed to the sled database
        let result = cache.get(b"key1")?;
        assert_eq!(result.unwrap().to_vec(), b"value1");

        let result = cache.get(b"key2")?;
        assert_eq!(result.unwrap().to_vec(), b"value2");

        let result = cache.get(b"key3")?;
        assert_eq!(result.unwrap().to_vec(), b"value3");

        // Check that kek4 was not flushed
        let result = cache.get(b"key4")?;
        assert_eq!(result.is_none(), true);

        // Check that we only have 1 hashmap
        let head_cache_guard = head_cache.read().unwrap();
        assert_eq!(head_cache_guard.len(), 1);

        Ok(())
    }
}
