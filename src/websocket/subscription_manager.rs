use crate::balancer::processing::CacheArgs;
use blake3::Hash;
use serde_json::Value;
use simd_json::to_vec;

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
// pub fn subscription_dispatch()
