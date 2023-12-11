use blake3::Hash;
use serde_json::Value;
use sled::Tree;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// We want to return the subscription id and insert it into a subtree
//
// If multiple nodes have made the same subscription request, we can just return
// the same subscription id to all of them.
// TODO: add referance counting !!!!!!!!!!!!!!!!!!!
pub fn insert_and_return_subscription(
	tx_hash: Hash,
	response: Value,
	tree: Tree,
) -> Result<String, Error> {
	let subscription_id = response["result"].clone();

	// Insert the subscription for this tx_hash into the subtree
	tree.insert(tx_hash.as_bytes(), &subscription_id.as_u64().unwrap().to_be_bytes())?;

	return Ok(subscription_id.to_string());
}
