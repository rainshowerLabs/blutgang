use crate::{
    rpc::error::RpcError,
    Rpc,
};
use std::sync::{
    Arc,
    RwLock,
};
use tokio::{
    select,
    time::{
        timeout,
        Duration,
    },
};

// Get the latest finalized block
pub async fn get_safe_block(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    finalized: &Arc<RwLock<u64>>,
    ttl: u64,
) -> Result<u64, RpcError> {
    let len = rpc_list.read().unwrap().len();
    let mut safe = 0;
    let mut threads = Vec::new();

    // Create a vector to store the futures of all RPC requests
    let mut rpc_futures = Vec::new();

    // Iterate over all RPCs
    for i in 0..len {
        let rpc_clone = rpc_list.read().unwrap()[i].clone();

        // Spawn a future for each RPC
        let rpc_future = async move {
            let a = rpc_clone.get_finalized_block();
            let result = timeout(Duration::from_millis(ttl.try_into().unwrap()), a).await;

            match result {
                Ok(response) => response.unwrap_or(0), // Handle timeout as 0
                Err(_) => 0,                           // Handle timeout as 0
            }
        };

        rpc_futures.push(rpc_future);
    }

    // Wait for all RPC futures concurrently and collect their results
    for rpc_future in rpc_futures {
        let result = tokio::spawn(rpc_future);
        threads.push(result);
    }

    // Collect the results using tokio::select!
    for _ in 0..len {
        select! {
            result = threads.pop().unwrap() => {
                if result.as_ref().unwrap() > &safe {
                    safe = result.unwrap();
                }
            }
        }
    }

    // Update the finalized block
    if safe > *finalized.read().unwrap() {
        *finalized.write().unwrap() = safe;
    }

    Ok(safe)
}
