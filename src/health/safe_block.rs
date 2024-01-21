use crate::{
    balancer::processing::CacheArgs,
    config::setup::WS_HEALTH_CHECK_USER_ID,
    rpc::{
        error::RpcError,
        types::{
            hex_to_decimal,
            Rpc,
        },
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

use std::sync::{
    Arc,
    RwLock,
};

use serde_json::Value;

use tokio::{
    sync::{
        broadcast,
        mpsc,
        watch,
    },
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

// Send a message subscribing to newHeads
async fn send_newheads_sub_message(
    user_id: u32,
    incoming_tx: &mpsc::UnboundedSender<WsconnMessage>,
    outgoing_rx: &broadcast::Receiver<IncomingResponse>,
    sub_data: &Arc<SubscriptionData>,
    cache_args: &CacheArgs,
) {
    let mut call = format!(
        r#"{{"jsonrpc":"2.0","method":"eth_subscribe","params":["newHeads"],"id":"{}"}}"#,
        user_id
    );
    let call: Value = unsafe { simd_json::from_str(&mut call).unwrap() };

    match execute_ws_call(
        call.clone(),
        user_id,
        &incoming_tx,
        outgoing_rx.resubscribe(),
        &sub_data,
        &cache_args,
    )
    .await
    {
        Ok(_) => {
            let _ = sub_data.subscribe_user(user_id, call);
        }
        Err(e) => {
            panic!(
                "FATAL: Error subscribing to newHeads in health check: {}",
                e
            )
        }
    };
}

// Subscribe to eth_subscribe("newHeads") and write to NamedBlocknumbers
pub async fn subscribe_to_new_heads(
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    outgoing_rx: broadcast::Receiver<IncomingResponse>,
    blocknum_tx: watch::Sender<u64>,
    sub_data: Arc<SubscriptionData>,
    cache_args: CacheArgs,
    ttl: u64,
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

    send_newheads_sub_message(user_id, &incoming_tx, &outgoing_rx, &sub_data, &cache_args).await;

    // New message == new head received. We can then update and process
    // everything associated with a new head block.
    loop {
        match timeout(Duration::from_millis((ttl as f64 * 1.5) as u64), rx.recv()).await {
            Ok(Some(msg)) => {
                if let RequestResult::Subscription(sub) = msg {
                    let mut nn_rwlock = cache_args.named_numbers.write().unwrap();
                    let a = hex_to_decimal(sub["params"]["result"]["number"].as_str().unwrap())
                        .unwrap();
                    println!("New head: {}", a);
                    let _ = blocknum_tx.send(a);
                    nn_rwlock.latest = a;
                }
            }
            Ok(None) => {
                // Handle the case where the channel is closed
                panic!("FATAL: Channel closed in newHeads subscription");
            }
            Err(_) => {
                // Handle the timeout case
                {
                    let mut nn_rwlock = cache_args.named_numbers.write().unwrap();
                    nn_rwlock.latest = 0;
                    incoming_tx.send(WsconnMessage::Reconnect()).unwrap();
                }
                println!("Timeout in newHeads subscription");
                send_newheads_sub_message(
                    user_id,
                    &incoming_tx,
                    &outgoing_rx,
                    &sub_data,
                    &cache_args,
                )
                .await;
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use tokio::sync::mpsc;
//     use std::sync::{Arc, RwLock};

//     fn mock_rpc() -> Rpc {
//         Rpc::new("http://localhost:3030".to_string(), Some("ws://localhost:3030".to_string()), 1000, 1, 10.0)
//     }

//     // #[tokio::test]
//     // async fn test_get_safe_block_empty_rpc_list() {
//     //     let rpc_list = Arc::new(RwLock::new(Vec::new()));
//     //     let (finalized_tx, _) = tokio::sync::watch::channel(0);
//     //     let named_numbers_rwlock = Arc::new(RwLock::new(NamedBlocknumbers::default()));
//     //     let ttl = 1000;

//     //     let result = get_safe_block(&rpc_list, &finalized_tx, &named_numbers_rwlock, ttl).await;
//     //     assert!(result.is_ok());
//     //     assert_eq!(result.unwrap(), 0);
//     // }

//     // #[tokio::test]
//     // async fn test_get_safe_block_with_rpc() {
//     //     let rpc_list = Arc::new(RwLock::new(vec![mock_rpc()]));
//     //     let (finalized_tx, _) = tokio::sync::watch::channel(0);
//     //     let named_numbers_rwlock = Arc::new(RwLock::new(NamedBlocknumbers::default()));
//     //     let ttl = 1000;

//     //     let result = get_safe_block(&rpc_list, &finalized_tx, &named_numbers_rwlock, ttl).await;
//     //     assert!(result.is_ok());
//     //     assert_eq!(result.unwrap(), 0); // Assuming mocked RPC returns 0 for finalized block
//     // }

//     #[tokio::test]
//     async fn test_subscribe_to_new_heads() {
//         let (incoming_tx, _) = mpsc::unbounded_channel();
//         let (outgoing_tx, outgoing_rx) = broadcast::channel(10);
//         let sub_data = Arc::new(SubscriptionData::new());
//         let cache_args = CacheArgs::default();
//         let ttl = 1000;

//         // Mock sending newHead subscription message
//         outgoing_tx.send(IncomingResponse {
//             content: serde_json::json!({
//                 "params": {
//                     "result": {
//                         "number": "0x1"
//                     }
//                 }
//             }),
//             node_id: 1,
//         }).unwrap();

//         tokio::spawn(async move {
//             subscribe_to_new_heads(incoming_tx, outgoing_rx, sub_data, cache_args, ttl).await;
//         });

//         // Allow for some processing time
//         tokio::time::sleep(Duration::from_millis(500)).await;

//         // Expectations and assertions would typically follow here,
//         // but they are limited due to the nature of `subscribe_to_new_heads` function's loop.
//     }
// }
