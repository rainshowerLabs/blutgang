use crate::{
    balancer::{
        format::get_block_number_from_request,
        selection::cache_rules::{
            cache_method,
            cache_result,
        },
    },
    database::{
        accept::db_insert,
        types::{
            GenericBytes,
            RequestBus,
        },
    },
    health::safe_block::NamedBlocknumbers,
    Rpc,
};

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use tokio::sync::watch;

use blake3::Hash;
use serde_json::Value;
use simd_json::to_vec;

#[derive(Clone)]
pub struct CacheArgs<K, V>
where
    K: GenericBytes,
    V: GenericBytes,
{
    pub finalized_rx: watch::Receiver<u64>,
    pub named_numbers: Arc<RwLock<NamedBlocknumbers>>,
    pub head_cache: Arc<RwLock<BTreeMap<u64, Vec<K>>>>,
    pub cache: RequestBus<K, V>,
}

impl CacheArgs<[u8; 32], Vec<u8>> {
    #[cfg(test)]
    /// **Note:** This should only be used for testing!
    pub fn default() -> Self {
        use crate::database_processing;

        use sled::{
            Config,
            Db,
        };

        use tokio::sync::mpsc;

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
//
// @makemake -- Here's an intermediate solution to step towards the above todo which
// uses a loose trait constraint `AsRef<str>` which is implemented for the method types.
pub fn can_cache<M: AsRef<str>>(method: M, result: &str) -> bool {
    cache_method(method) && cache_result(result)
}

/// Check if we should cache the query, and if so cache it in the DB
pub async fn cache_query<K, V>(rx: &str, method: Value, tx_hash: Hash, cache_args: &CacheArgs<K, V>)
where
    K: GenericBytes + From<[u8; 32]>,
    V: GenericBytes + From<Vec<u8>>,
{
    if can_cache(method.to_string(), rx) {
        // Insert the response hash into the head_cache
        let num = get_block_number_from_request(method, &cache_args.named_numbers);

        // Insert the key of the request we made into our `head_cache`
        // so we can invalidate it and remove it from the DB if it reorgs.
        if let Some(num) = num {
            if num > *cache_args.finalized_rx.borrow() {
                let mut head_cache = cache_args.head_cache.write().unwrap();
                head_cache
                    .entry(num)
                    .or_default()
                    .push(tx_hash.as_bytes().to_owned().into());
            }

            // Replace the id with Value::Null and insert the request.
            //
            // In some cases the response might not contain an ID like in
            // https://github.com/rainshowerLabs/blutgang/issues/88.
            // In this case we just skip inserting it into the DB as its an error.
            //
            // TODO: kinda cringe how we do this gymnasctics of changing things back and forth
            //
            // simd-json parses in place and rewrites its input, so parse a copy: `rx` is
            // still sent to the client.
            let Ok(mut rx_value) =
                simd_json::serde::from_slice::<Value>(&mut rx.as_bytes().to_vec())
            else {
                return;
            };
            if let Some(id) = rx_value.get_mut("id") {
                *id = Value::Null;
            } else {
                return;
            }

            drop(
                db_insert(
                    &cache_args.cache.clone(),
                    tx_hash.as_bytes().to_owned().into(),
                    to_vec(&rx_value).unwrap().into(),
                )
                .await,
            );
        }
    }
}

/// Updates the latency of the RPC with the given [`Rpc::id`], given the time it took for
/// a request to complete.
///
/// Does nothing if the RPC has left the list since the request was sent.
pub fn update_rpc_latency(rpc_list: &Arc<RwLock<Vec<Rpc>>>, rpc_id: usize, time: Duration) {
    let mut rpc_list_guard = rpc_list.write().unwrap_or_else(|e| {
        // Handle the case where the RwLock is poisoned
        e.into_inner()
    });

    if let Some(rpc) = rpc_list_guard.iter_mut().find(|rpc| rpc.id() == rpc_id) {
        rpc.update_latency(time.as_nanos() as f64);
        tracing::info!("LA {}", rpc.status.latency);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        db_get,
        rpc::method::EthRpcMethod,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn test_can_cache() {
        assert!(can_cache(
            EthRpcMethod::GetBlockByNumber,
            r#"{"result": "0x1"}"#
        ));
        assert!(!can_cache(EthRpcMethod::Subscribe, r#"{"result": "0x1"}"#));
    }

    #[test]
    fn test_dont_cache_infura_err() {
        assert!(!can_cache(
            r#"{"method": "eth_getBlockByNumber", "params": ["0x10", false]}"#,
            r#"{ "code": -32005, "data": { "see": "https://infura.io/dashboard" }, "message": "daily request count exceeded, request rate limited" }, payload={ "id": 12449, "jsonrpc": "2.0", "method": "eth_blockNumber", "params": [  ] }"#
        ));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cache_query() {
        let cache_args = CacheArgs::default();
        let rx = r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#;
        let method = json!({"method": EthRpcMethod::GetBlockByNumber, "params": ["0x10", false]});
        let tx_hash = blake3::hash(method.to_string().as_bytes());

        cache_query(rx, method.clone(), tx_hash, &cache_args).await;

        let cached_value = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned())
            .unwrap()
            .unwrap();
        let cached_str = std::str::from_utf8(&cached_value).unwrap();
        assert_eq!(cached_str, r#"{"id":null,"jsonrpc":"2.0","result":"0x1"}"#);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cache_query_leaves_response_untouched() {
        let cache_args = CacheArgs::default();
        // The response is sent to the client after caching it, so escapes must survive.
        let response = r#"{"jsonrpc":"2.0","result":"say \"hi\"\nto é","id":1}"#;
        let rx = response.to_string();
        let method = json!({"method": EthRpcMethod::GetBlockByNumber, "params": ["0x10", false]});
        let tx_hash = blake3::hash(method.to_string().as_bytes());

        cache_query(&rx, method, tx_hash, &cache_args).await;

        assert_eq!(rx, response);
        let cached_value = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned())
            .unwrap()
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&cached_value).unwrap(),
            r#"{"id":null,"jsonrpc":"2.0","result":"say \"hi\"\nto é"}"#
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cache_query_skips_invalid_json() {
        let cache_args = CacheArgs::default();
        let method = json!({"method": EthRpcMethod::GetBlockByNumber, "params": ["0x10", false]});
        let tx_hash = blake3::hash(method.to_string().as_bytes());

        cache_query("<html>502 Bad Gateway</html>", method, tx_hash, &cache_args).await;

        let cached_value = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned()).unwrap();
        assert!(cached_value.is_none());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cache_infura_error_query() {
        let cache_args = CacheArgs::default();
        let rx = r#"{ "code": -32005, "data": { "see": "https://infura.io/dashboard" }, "message": "daily request count exceeded, request rate limited" }, payload={ "id": 12449, "jsonrpc": "2.0", "method": "eth_blockNumber", "params": [  ] }"#;
        let method = json!({"method": EthRpcMethod::GetBlockByNumber, "params": ["0x10", false]});
        let tx_hash = blake3::hash(method.to_string().as_bytes());

        cache_query(rx, method.clone(), tx_hash, &cache_args).await;

        let cached_value = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned()).unwrap();
        assert!(
            cached_value.is_none(),
            "got cached value for transaction that should have failed"
        );
    }

    fn test_rpc(name: &str) -> Rpc {
        Rpc::new(
            format!("http://{name}").parse().unwrap(),
            Some(format!("ws://{name}").parse().unwrap()),
            0,
            0,
            1.0,
        )
    }

    #[tokio::test]
    async fn test_update_rpc_latency() {
        let rpc = test_rpc("test_rpc");
        let id = rpc.id();
        let rpc_list = Arc::new(RwLock::new(vec![rpc]));
        update_rpc_latency(&rpc_list, id, Duration::from_nanos(100));

        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].status.latency, 100.0);
    }

    #[tokio::test]
    async fn test_update_rpc_latency_with_multiple_rpcs() {
        let rpcs = vec![test_rpc("test_rpc1"), test_rpc("test_rpc2")];
        let id = rpcs[1].id();
        let rpc_list = Arc::new(RwLock::new(rpcs));
        update_rpc_latency(&rpc_list, id, Duration::from_nanos(200));

        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].status.latency, 0.0);
        assert_eq!(rpcs[1].status.latency, 200.0);
    }

    #[tokio::test]
    async fn test_update_rpc_latency_with_unknown_id() {
        let rpc = test_rpc("test_rpc");
        let unknown = test_rpc("removed_rpc").id();
        let rpc_list = Arc::new(RwLock::new(vec![rpc]));
        update_rpc_latency(&rpc_list, unknown, Duration::from_nanos(300));

        // The RPC that served the request is gone, so no other RPC should be charged.
        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].status.latency, 0.0);
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
    async fn test_update_rpc_latency_after_list_shifts() {
        let rpcs = vec![test_rpc("test_rpc1"), test_rpc("test_rpc2")];
        let id = rpcs[1].id();
        let rpc_list = Arc::new(RwLock::new(rpcs));

        // The first RPC is removed while a request to the second is in flight.
        rpc_list.write().unwrap().remove(0);
        update_rpc_latency(&rpc_list, id, Duration::from_nanos(500));

        let rpcs = rpc_list.read().unwrap();
        assert_eq!(rpcs[0].id(), id);
        assert_eq!(rpcs[0].status.latency, 500.0);
    }
}
