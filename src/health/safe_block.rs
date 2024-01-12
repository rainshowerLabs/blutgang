use crate::{
    rpc::{
        error::RpcError,
        types::hex_to_decimal,
    },
    websocket::types::{
        WsChannelErr,
        WsconnMessage,
    },
    ws_conn_manager,
    Rpc,
    config::cache_setup::WS_HEALTH_CHECK_USER_ID,
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

impl NamedBlocknumbers {
    #[allow(dead_code)] // allowed for tests
    pub fn defualt() -> NamedBlocknumbers {
        NamedBlocknumbers {
            latest: 0,
            earliest: 0,
            safe: 0,
            finalized: 0,
            pending: 0,
            number: 0,
        }
    }
}

// Get the latest finalized block
pub async fn get_safe_block(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    finalized_tx: &tokio::sync::watch::Sender<u64>,
    named_numbers_rwlock: &Arc<RwLock<NamedBlocknumbers>>,
    ttl: u64,
) -> Result<u64, RpcError> {
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
    let mut nn_rwlock = named_numbers_rwlock.write().unwrap();
    nn_rwlock.finalized = safe;

    Ok(safe)
}

// Subscribe to eth_subscribe("newHeads") and write to NamedBlocknumbers
pub async fn subscribe_to_new_heads(
) {
    // We basically have to create a new system-only user for subscribing to newHeads
    
    // Create channels for message send/receiving
    let (tx, mut rx) = mpsc::unbounded_channel::<RequestResult>();

    // Generate an id for our user
    //
    // We use this to identify which requests are for us
    let user_id = WS_HEALTH_CHECK_USER_ID;

    // Add the user to the sink map
    println!("\x1b[35mInfo:\x1b[0m Adding user {} to sink map", user_id);
    let user_data = UserData {
        message_channel: tx.clone(),
    };
    sub_data.add_user(user_id, user_data);


    
}
