use crate::{
    balancer::processing::CacheArgs,
    websocket::{
        error::Error,
        types::{
            RequestResult,
            SubscriptionData,
        },
    },
};
use blake3::Hash;

use serde_json::Value;
use simd_json::to_vec;
use std::sync::Arc;
use tokio::sync::broadcast;

// We want to return the subscription id and insert it into a subtree
//
// If multiple nodes have made the same subscription request, we can just return
// the same subscription id to all of them.
// TODO: add referance counting !!!!!!!!!!!!!!!!!!!
pub fn insert_and_return_subscription(
    tx_hash: Hash,
    mut response: Value,
    cache_args: &CacheArgs,
) -> Result<Value, Error> {
    response["id"] = Value::Null;

    let tree = cache_args.cache.open_tree("subscriptions")?;

    // Insert the subscription for this tx_hash into the subtree
    let _ = tree.insert(tx_hash.as_bytes(), to_vec(&response).unwrap().as_slice());

    Ok(response)
}

// Sends all subscriptions to their relevant nodes
pub fn subscription_dispatcher(
    mut rx: broadcast::Receiver<Value>,
    sub_data: Arc<SubscriptionData>,
) {
    tokio::spawn(async move {
        loop {
            // Receive the WS response
            let response = rx.recv().await.unwrap();
            // println!("subscription_dispatcher: received response: {}", response);

            // Check if its a subscription
            if response["method"] != "eth_subscription" {
                continue;
            }

            #[cfg(feature = "debug-verbose")]
            println!(
                "subscription_dispatcher: received subscription: {}",
                response
            );

            // Get the subscription id
            // TODO: this is retarded???
            let resp_clone = response.clone();
            let id = response["params"]["subscription"].as_str().unwrap();

            // Send the response to all the users
            // TODO: nodeid is temp
            match sub_data
                .dispatch_to_subscribers(id, 0, &RequestResult::Subscription(resp_clone))
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
    use super::*;
    use crate::{
        health::safe_block::NamedBlocknumbers,
        // websocket::types::UserData,
    };
    use std::collections::BTreeMap;
    use std::sync::RwLock;
    use tokio::sync::watch;

    // use std::str::FromStr;
    // use tokio::sync::{
    //     broadcast,
    //     mpsc,
    // };

    // fn setup_subscription_data() -> (
    //     Arc<SubscriptionData>,
    //     broadcast::Sender<Value>,
    //     broadcast::Receiver<Value>,
    // ) {
    //     let (tx, rx) = broadcast::channel(10);
    //     let sub_data = Arc::new(SubscriptionData::new());
    //     (sub_data, tx, rx)
    // }

    fn setup_cache_args() -> (CacheArgs, watch::Sender<u64>) {
        let config = sled::Config::default();
        let (finalized_tx, finalized_rx) = watch::channel(0);
        config.clone().temporary(true);
        let b = NamedBlocknumbers::default();
        let mut map = BTreeMap::new();
        map.insert(u64::MAX, vec!["a".to_string()]); //retarded
        let a = CacheArgs {
            cache: Arc::new(config.open().unwrap()),
            named_numbers: Arc::new(RwLock::new(b)),
            finalized_rx: finalized_rx.clone(),
            head_cache: Arc::new(RwLock::new(map)),
        };

        return (a, finalized_tx);
    }

    #[tokio::test]
    async fn test_insert_and_return_subscription() {
        let (cache_args, _a) = setup_cache_args();
        let mut strr = r#"{"result":"1", "id":"2"}"#.to_string();
        let response: Value = unsafe { simd_json::from_str(&mut strr).unwrap() };
        let tx_hash = blake3::hash(response.to_string().as_bytes());

        let result =
            insert_and_return_subscription(tx_hash, response.clone(), &cache_args).unwrap();

        assert!(result["id"].is_null());
        let mut stored_value = cache_args
            .cache
            .open_tree("subscriptions")
            .unwrap()
            .get(tx_hash.as_bytes())
            .unwrap()
            .unwrap();
        let mut stored_value: Value = simd_json::from_slice(&mut stored_value).unwrap();
        assert!(stored_value["id"].is_null());

        stored_value["id"] = response["id"].clone();

        assert_eq!(stored_value, response);
    }

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
