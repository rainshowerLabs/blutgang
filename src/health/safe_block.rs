use crate::{
    balancer::processing::CacheArgs,
    config::cache_setup::WS_HEALTH_CHECK_USER_ID,
    rpc::{
        error::RpcError,
        types::Rpc,
    },
    websocket::{
        client::execute_ws_call,
        types::{
            IncomingResponse,
            RequestResult,
            SubscriptionData,
            UserData,
            WsconnMessage,
        },
    },
};

use serde_json::Value;
use std::sync::{
    Arc,
    RwLock,
};
use std::unimplemented;
use tokio::sync::broadcast;
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
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    outgoing_rx: broadcast::Receiver<IncomingResponse>,
    sub_data: Arc<SubscriptionData>,
    cache_args: CacheArgs,
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

    let mut call = format!(
        r#"{{"jsonrpc":"2.0","method":"eth_subscribe","params":["newHeads"],"id":"{}"}}"#,
        user_id
    );
    let call = unsafe { simd_json::from_str(&mut call).unwrap() };

    // Send a message subscribing to newHeads
    let subscription_id = match execute_ws_call(
        call,
        user_id,
        &incoming_tx,
        outgoing_rx.resubscribe(),
        &sub_data,
        &cache_args,
    )
    .await
    {
        Ok(mut rax) => {
            // extract the id from rax and return it
            let json: Value = unsafe { simd_json::from_str(&mut rax).unwrap() };
            // TODO: error handle this
            json["params"]["0"].as_str().unwrap().to_string()
        }
        Err(e) => panic!("Error subscribing to newHeads in health check: {}", e),
    };

    while let Some(msg) = rx.recv().await {
        // Forward the message to the best available RPC
        //
        // If we received a subscription, just send it to the client
        match msg {
            RequestResult::Subscription(sub) => {
                unimplemented!();
            }
            _ => {}
        }
    }
}
