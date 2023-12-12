use crate::{
    balancer::processing::CacheArgs,
    rpc::types::hex_to_decimal,
};
use blake3::Hash;
use dashmap::DashMap;
use serde_json::Value;
use simd_json::to_vec;
use std::println;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
pub enum RequestResult {
    Call(Value),
    Subscription(Value),
}

impl From<RequestResult> for Value {
    fn from(req: RequestResult) -> Self {
        match req {
            RequestResult::Call(call) => call,
            RequestResult::Subscription(sub) => sub,
        }
    }
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
    sink_map: Arc<DashMap<u64, mpsc::UnboundedSender<RequestResult>>>,
    subscribed_users: Arc<DashMap<u64, DashMap<u64, bool>>>,
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
            let id = hex_to_decimal(id).unwrap();

            // Get all the users that are subscribed to this subscription
            let a = subscribed_users.clone();
            let users = a.get(&id);
            let users = users.as_deref();

            // If there are no users, we can skip this
            // TODO: UNSUBSCRIBE!!!
            if users.is_none() || users.unwrap().is_empty() {
                continue;
            }

            let users = users.unwrap();

            // Send the response to all the users
            //
            // `users` is a map of <subscription id, <user, is subscribed>>
            //  we want to iter over the users and send them the response
            for user in users.iter() {
                let user = user.key();

                // Get the user's channel
                let tx = sink_map.get(user).unwrap();

                println!(
                    "\x1b[35mInfo:\x1b[0m Sending subscription to user {}",
                    user
                );
                // Send the response
                let _ = tx.send(RequestResult::Subscription(response.clone()));
            }
        }
    });
}
