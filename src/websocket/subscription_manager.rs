use crate::websocket::types::{
    IncomingResponse,
    RequestResult,
    SubscriptionData,
};

use std::sync::Arc;
use tokio::sync::broadcast;

// Sends all subscriptions to their relevant nodes
pub fn subscription_dispatcher(
    mut rx: broadcast::Receiver<IncomingResponse>,
    sub_data: Arc<SubscriptionData>,
) {
    tokio::spawn(async move {
        loop {
            // Receive the WS response
            let response = rx.recv().await.unwrap();

            // Check if its a subscription
            if response.content["method"] != "eth_subscription" {
                continue;
            }

            #[cfg(feature = "debug-verbose")]
            println!(
                "subscription_dispatcher: received subscription: {}",
                response.content
            );

            // Get the subscription id
            // TODO: this is retarded???
            let resp_clone = response.clone();
            let id = response.content["params"]["subscription"].as_str().unwrap();

            // Send the response to all the users
            match sub_data
                .dispatch_to_subscribers(
                    id,
                    response.node_id,
                    &RequestResult::Subscription(resp_clone.content),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    println!(
                        "\x1b[31mErr:\x1b[0m Fatal error while trying to send subscriptions: {}",
                        e
                    )
                }
            };
        }
    });
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use crate::balancer::processing::CacheArgs;
    // use crate::websocket::client::execute_ws_call;
    // use crate::Rpc;
    // use crate::WsconnMessage;
    // use serde_json::json;
    // use std::sync::RwLock;
    // use std::{
    //     sync::Arc,
    //     time::Duration,
    // };
    // use tokio::sync::{
    //     broadcast,
    //     mpsc,
    // };

    // fn setup_test_environment() -> (
    //     Arc<RwLock<Vec<Rpc>>>,
    //     mpsc::UnboundedSender<WsconnMessage>,
    //     mpsc::UnboundedReceiver<WsconnMessage>,
    //     broadcast::Sender<IncomingResponse>,
    //     broadcast::Receiver<IncomingResponse>,
    //     Arc<SubscriptionData>,
    // ) {
    //     let rpc_list = Arc::new(RwLock::new(vec![Rpc::default(), Rpc::default()]));
    //     let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
    //     let (broadcast_tx, broadcast_rx) = broadcast::channel(10);
    //     let sub_data = Arc::new(SubscriptionData::new());

    //     (
    //         rpc_list,
    //         incoming_tx,
    //         incoming_rx,
    //         broadcast_tx,
    //         broadcast_rx,
    //         sub_data,
    //     )
    // }

    // #[tokio::test]
    // async fn test_subscribe_and_forward_incoming_subscriptions() {
    //     let (_rpc_list, incoming_tx, mut incoming_rx, broadcast_tx, _broadcast_rx, sub_data) =
    //         setup_test_environment();

    //     // Mock subscription call
    //     let subscription_call = json!({
    //         "jsonrpc": "2.0",
    //         "method": "eth_subscribe",
    //         "params": ["newHeads"],
    //         "id": 1
    //     });

    //     // Mock user
    //     let user_id = 123;

    //     // Execute subscription call
    //     let result = execute_ws_call(
    //         subscription_call.clone(),
    //         user_id,
    //         &incoming_tx,
    //         broadcast_tx.subscribe(),
    //         &sub_data,
    //         &CacheArgs::default(),
    //     )
    //     .await;

    //     assert!(result.is_ok());

    //     // Simulate incoming subscription message
    //     let incoming_subscription = json!({
    //         "method": "eth_subscription",
    //         "params": {
    //             "subscription": "0x1a2b3c",
    //             "result": {
    //                 "blockNumber": "0x10"
    //             }
    //         }
    //     });

    //     // Broadcast incoming subscription message
    //     broadcast_tx
    //         .send(IncomingResponse {
    //             node_id: 0,
    //             content: incoming_subscription.clone(),
    //         })
    //         .unwrap();

    //     // Simulate dispatching the incoming subscriptions
    //     subscription_dispatcher(broadcast_tx.subscribe(), Arc::clone(&sub_data));

    //     // Allow time for async operations to complete
    //     tokio::time::sleep(Duration::from_millis(100)).await;

    //     // Check if user has received the subscription
    //     //
    //     // Attempt to receive a message from the user's channel
    //     let received = incoming_rx.try_recv().unwrap();
    //     let received = match received {
    //         WsconnMessage::Message(sub, _) => sub == incoming_subscription,
    //         _ => false,
    //     };

    //     assert!(
    //         received,
    //         "User did not receive the expected subscription message"
    //     );
    // }

    // fn setup_subscription_data() -> (
    //     Arc<SubscriptionData>,
    //     broadcast::Sender<Value>,
    //     broadcast::Receiver<Value>,
    // ) {
    //     let (tx, rx) = broadcast::channel(10);
    //     let sub_data = Arc::new(SubscriptionData::new());
    //     (sub_data, tx, rx)
    // }

    // fn setup_cache_args() -> (CacheArgs, watch::Sender<u64>) {
    //     let config = sled::Config::default();
    //     let (finalized_tx, finalized_rx) = watch::channel(0);
    //     config.clone().temporary(true);
    //     let b = NamedBlocknumbers::default();
    //     let mut map = BTreeMap::new();
    //     map.insert(u64::MAX, vec!["a".to_string()]); //retarded
    //     let a = CacheArgs {
    //         cache: Arc::new(config.open().unwrap()),
    //         named_numbers: Arc::new(RwLock::new(b)),
    //         finalized_rx: finalized_rx.clone(),
    //         head_cache: Arc::new(RwLock::new(map)),
    //     };

    //     return (a, finalized_tx);
    // }

    // TODO: fix tests

    // #[tokio::test]
    // async fn test_subscription_dispatcher() {
    //     let (sub_data, tx, rx) = setup_subscription_data();
    //     let (user_tx, mut user_rx) = mpsc::unbounded_channel();

    //     let user_id = 123;
    //     sub_data.users.write().expect("REASON").insert(
    //         user_id,
    //         UserData {
    //             message_channel: user_tx,
    //         },
    //     );

    //     let subscription_id = 456;
    //     sub_data
    //         .subscriptions
    //         .write()
    //         .expect("REASON")
    //         .insert(subscription_id, std::iter::once(user_id).collect());

    //     let response =
    //         Value::from_str(r#"{"method":"eth_subscription", "params":{"subscription":"0x1c"}}"#)
    //             .unwrap();
    //     tx.send(response.clone()).unwrap();

    //     subscription_dispatcher(rx, sub_data.clone());

    //     if let Some(RequestResult::Subscription(sub_response)) = user_rx.recv().await {
    //         assert_eq!(sub_response, response);
    //     } else {
    //         panic!("Expected to receive a subscription message");
    //     }
    // }

    // #[tokio::test]
    // async fn test_insert_and_return_subscription_with_invalid_json() {
    //     let (cache_args, _a) = setup_cache_args();
    //     let response = Value::String("invalid json".to_string());
    //     let tx_hash = blake3::hash(b"invalid json test");

    //     let result = insert_and_return_subscription(tx_hash, response, &cache_args);
    //     assert!(result.is_err());
    // }

    // #[tokio::test]
    // async fn test_subscription_dispatcher_with_invalid_method() {
    //     let (sub_data, tx, rx) = setup_subscription_data();
    //     let response = Value::from_str(r#"{"method":"invalid_method"}"#).unwrap();
    //     tx.send(response.clone()).unwrap();

    //     subscription_dispatcher(rx, sub_data.clone());

    //     let (user_tx, mut user_rx) = mpsc::unbounded_channel::<RequestResult>();
    //     let user_id = 789;
    //     sub_data.users.write().expect("REASON").insert(
    //         user_id,
    //         UserData {
    //             message_channel: user_tx,
    //         },
    //     );

    //     assert!(user_rx.recv().await.is_none());
    // }

    // #[tokio::test]
    // async fn test_subscription_dispatcher_with_no_subscribers() {
    //     let (sub_data, tx, rx) = setup_subscription_data();
    //     let response =
    //         Value::from_str(r#"{"method":"eth_subscription", "params":{"subscription":"0x1c"}}"#)
    //             .unwrap();
    //     tx.send(response.clone()).unwrap();

    //     subscription_dispatcher(rx, sub_data.clone());

    //     // No subscribers added to sub_data
    //     let (user_tx, mut user_rx) = mpsc::unbounded_channel::<RequestResult>();
    //     sub_data.users.write().expect("REASON").insert(
    //         123,
    //         UserData {
    //             message_channel: user_tx,
    //         },
    //     );

    //     assert!(user_rx.recv().await.is_none());
    // }

    // #[tokio::test]
    // async fn test_subscription_dispatcher_with_hex_to_decimal_error() {
    //     let (sub_data, tx, rx) = setup_subscription_data();
    //     let response = Value::from_str(
    //         r#"{"method":"eth_subscription", "params":{"subscription":"invalid_hex"}}"#,
    //     )
    //     .unwrap();
    //     tx.send(response.clone()).unwrap();

    //     subscription_dispatcher(rx, sub_data.clone());

    //     let (user_tx, mut user_rx) = mpsc::unbounded_channel::<RequestResult>();
    //     sub_data.users.write().expect("REASON").insert(
    //         123,
    //         UserData {
    //             message_channel: user_tx,
    //         },
    //     );

    //     // Expect no message due to hex_to_decimal error
    //     assert!(user_rx.recv().await.is_none());
    // }

    // // Additional test for error handling in subscription dispatcher
    // #[tokio::test]
    // async fn test_subscription_dispatcher_error_handling() {
    //     let (sub_data, tx, mut rx) = setup_subscription_data();
    //     let response =
    //         Value::from_str(r#"{"method":"eth_subscription", "params":{"subscription":"0x1c"}}"#)
    //             .unwrap();
    //     tx.send(response.clone()).unwrap();

    //     subscription_dispatcher(rx.resubscribe(), sub_data.clone());

    //     // Close the channel to simulate an error in the dispatcher
    //     drop(tx);
    //     // Ensure that the dispatcher loop breaks and does not panic
    //     assert!(rx.recv().await.is_err());
    // }
}
