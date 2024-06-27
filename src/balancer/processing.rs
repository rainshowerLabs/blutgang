use crate::{
    balancer::{
        format::get_block_number_from_request,
        selection::cache_rules::{
            cache_method,
            cache_result,
        },
    },
    database::types::{
        DbRequest,
        RequestBus,
        RequestKind,
    },
    db_insert,
    health::safe_block::NamedBlocknumbers,
    Rpc,
};

use std::{
    collections::BTreeMap,
    println,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use tokio::sync::{
    oneshot,
    watch,
};

use blake3::Hash;
use serde_json::Value;
use simd_json::to_vec;

#[derive(Clone)]
pub struct CacheArgs {
    pub finalized_rx: watch::Receiver<u64>,
    pub named_numbers: Arc<RwLock<NamedBlocknumbers>>,
    pub head_cache: Arc<RwLock<BTreeMap<u64, Vec<String>>>>,
    pub cache: RequestBus,
}

impl CacheArgs {
    #[cfg(test)]
    /// **Note:** This should only be used for testing!
    pub fn default() -> Self {
        use crate::database_processing;

        use sled::{
            Config,
            Db,
        };

        let cache = Config::tmp().unwrap();
        let cache = Db::open_with_config(&cache).unwrap();

        let (db_tx, db_rx) = mpsc::unbounded_channel();
        tokio::task::spawn(database_processing(db_rx, cache));

        CacheArgs {
            finalized_rx: watch::channel(0).1,
            named_numbers: Arc::new(RwLock::new(NamedBlocknumbers::default())),
            head_cache: Arc::new(RwLock::new(BTreeMap::new())),
            cache: db_tx,
        }
    }
}

// TODO: we should find a way to check values directly and not convert Value to str
pub fn can_cache(method: &str, result: &str) -> bool {
    if cache_method(method) && cache_result(result) {
        return true;
    }
    false
}

/// Check if we should cache the querry, and if so cache it in the DB
pub fn cache_querry(rx: &mut str, method: Value, tx_hash: Hash, cache_args: &CacheArgs) {
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

            let _ = db_insert!(
                cache_args.cache,
                tx_hash.as_bytes().to_vec(),
                to_vec(&rx_value).unwrap().as_slice().into()
            );
        }
    }
}

/// Updates the latency of an RPC node given an rpc list, its position, and the time it took for
/// a request to complete.
pub fn update_rpc_latency(rpc_list: &Arc<RwLock<Vec<Rpc>>>, rpc_position: usize, time: Duration) {
    let mut rpc_list_guard = rpc_list.write().unwrap_or_else(|e| {
        // Handle the case where the RwLock is poisoned
        e.into_inner()
    });

    // Handle weird edge cases ¯\_(ツ)_/¯
    if !rpc_list_guard.is_empty() {
        let index = if rpc_position >= rpc_list_guard.len() {
            rpc_list_guard.len() - 1
        } else {
            rpc_position
        };
        rpc_list_guard[index].update_latency(time.as_nanos() as f64);
        rpc_list_guard[index].last_used = time.as_micros();
        println!("LA {}", rpc_list_guard[index].status.latency);
    }
}

#[cfg(test)]
mod tests {
    use crate::db_get;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_can_cache() {
        assert!(can_cache("eth_getBlockByNumber", r#"{"result": "0x1"}"#));
        assert!(!can_cache("eth_subscribe", r#"{"result": "0x1"}"#));
    }

    #[tokio::test]
    async fn test_cache_querry() {
        let cache_args = CacheArgs::default();
        let mut rx = r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#.to_string();
        let method = json!({"method": "eth_getBlockByNumber", "params": ["latest", false]});
        let tx_hash = blake3::hash(method.to_string().as_bytes());

        cache_querry(&mut rx, method.clone(), tx_hash, &cache_args);

        let cached_value = db_get!(cache_args.cache, tx_hash.as_bytes().to_vec())
            .unwrap()
            .unwrap();
        let cached_str = std::str::from_utf8(&cached_value).unwrap();
        assert_eq!(cached_str, r#"{"id":null,"jsonrpc":"2.0","result":"0x1"}"#);
    }

    #[tokio::test]
    async fn test_update_rpc_latency() {
        let rpc_list = Arc::new(RwLock::new(vec![Rpc::new(
            "http://test_rpc".to_string(),
            Some("ws://test_rpc".to_string()),
            0,
            0,
            1.0,
        )]));
        update_rpc_latency(&rpc_list, 0, Duration::from_nanos(100));

        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].status.latency, 100.0);
    }

    #[tokio::test]
    async fn test_update_rpc_latency_with_multiple_rpcs() {
        let rpc_list = Arc::new(RwLock::new(vec![
            Rpc::new(
                "http://test_rpc1".to_string(),
                Some("ws://test_rpc1".to_string()),
                0,
                0,
                1.0,
            ),
            Rpc::new(
                "http://test_rpc2".to_string(),
                Some("ws://test_rpc2".to_string()),
                0,
                0,
                1.0,
            ),
        ]));
        update_rpc_latency(&rpc_list, 1, Duration::from_nanos(200));

        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[1].status.latency, 200.0);
    }

    #[tokio::test]
    async fn test_update_rpc_latency_with_invalid_position() {
        let rpc_list = Arc::new(RwLock::new(vec![Rpc::new(
            "http://test_rpc".to_string(),
            Some("ws://test_rpc".to_string()),
            0,
            0,
            1.0,
        )]));
        update_rpc_latency(&rpc_list, 10, Duration::from_nanos(300));

        // Since the position is invalid, it should update the last available RPC
        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].status.latency, 300.0);
    }

    #[tokio::test]
    async fn test_update_rpc_latency_with_empty_rpc_list() {
        let rpc_list = Arc::new(RwLock::new(Vec::new()));
        update_rpc_latency(&rpc_list, 0, Duration::from_nanos(400));

        // With an empty RPC list, there should be no panic and no update
        let rpcs = rpc_list.read().unwrap();
        assert!(rpcs.is_empty());
    }

    #[tokio::test]
    async fn test_update_rpc_latency_edge_cases() {
        let rpc_list = Arc::new(RwLock::new(vec![
            Rpc::new(
                "http://test_rpc1".to_string(),
                Some("ws://test_rpc1".to_string()),
                0,
                0,
                1.0,
            ),
            Rpc::new(
                "http://test_rpc2".to_string(),
                Some("ws://test_rpc2".to_string()),
                0,
                0,
                1.0,
            ),
        ]));

        // Test edge case where rpc_position is equal to rpc_list length
        update_rpc_latency(&rpc_list, 2, Duration::from_nanos(500));
        let rpcs = rpc_list.read().unwrap();
        assert_eq!(
            rpcs[1].status.latency, 500.0,
            "Should update the last RPC in the list"
        );
    }
}
