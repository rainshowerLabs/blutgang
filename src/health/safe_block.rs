use crate::{
    rpc::error::RpcError,
    Rpc,
};
use std::sync::{
    Arc,
    RwLock,
};
use tokio::{
    sync::mpsc,
    time::{
        timeout,
        Duration,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NamedBlocknumbers {
    pub latest: u64,
    pub earliest: u64,
    pub safe: u64,
    pub finalized: u64,
    pub pending: u64,
    pub number: u64,
}

// Get the latest finalized block
pub async fn get_safe_block(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    finalized_tx: &tokio::sync::watch::Sender<u64>,
    ttl: u64,
) -> Result<NamedBlocknumbers, RpcError> {
    let len = rpc_list.read().unwrap().len();
    let mut safe = 0;

    // If len == 0 return 0
    if len == 0 {
        return Ok(safe);
    }

    // Create a vector to store the futures of all RPC requests
    let mut rpc_futures = Vec::new();

    // Create a channel for collecting results
    let (tx, mut rx) = mpsc::channel(len);

    // Iterate over all RPCs
    for i in 0..len {
        let rpc_clone = rpc_list.read().unwrap()[i].clone();
        let tx = tx.clone(); // Clone the sender for this RPC

        // Spawn a future for each RPC
        let rpc_future = async move {
            let a = rpc_clone.get_finalized_block();
            let result = timeout(Duration::from_millis(ttl), a).await;

            let reported_finalized = match result {
                Ok(response) => response.unwrap(), // Handle timeout as 0
                Err(_) => 0,                       // Handle timeout as 0
            };

            // Send the result to the main thread through the channel
            tx.send(reported_finalized)
                .await
                .expect("head check: Channel send error");
        };

        rpc_futures.push(rpc_future);
    }

    // Wait for all RPC futures concurrently
    for rpc_future in rpc_futures {
        tokio::spawn(rpc_future);
    }

    // Collect the results in order from the channel
    for _ in 0..len {
        if let Some(result) = rx.recv().await {
            if result > safe {
                safe = result;
            }
        }
    }

    // Send new blocknumber if modified
    let send_if_changed = |number: &mut u64| {
        if number != &safe {
            *number = safe;
            return true;
        }
        false
    };

    finalized_tx.send_if_modified(send_if_changed);

    // println!("Safe block: {}", safe);

    // Return as NamedBlocknumbers
    let mut nn = NamedBlocknumbers::default();
    nn.safe = safe;

    Ok(nn)
}
