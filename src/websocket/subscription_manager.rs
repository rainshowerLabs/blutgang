use crate::{
    balancer::processing::CacheArgs,
    rpc::types::hex_to_decimal,
    websocket::types::{
        RequestResult,
        SubscriptionData,
    },
};
use blake3::Hash;

use serde_json::Value;
use simd_json::to_vec;
use std::{
    sync::Arc,
};
use tokio::sync::{
    broadcast,
};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

enum AuthorityMessage {
    CheckExists(Value, u64),
    AddSubscription(Value, u64),
}

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

            // Check if its a subscription
            if response["method"] != "eth_subscription" {
                continue;
            }

            // Get the subscription id
            let id = response["params"]["subscription"].as_str().unwrap();
            // TODO: this can be a string
            let id = hex_to_decimal(id).unwrap();

            // Send the response to all the users
            sub_data.dispatch_to_subscribers(id, &RequestResult::Subscription(response));
        }
    });
}
