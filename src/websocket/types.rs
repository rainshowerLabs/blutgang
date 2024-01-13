use std::{
    collections::{
        HashMap,
        HashSet,
    },
    println,
    sync::{
        Arc,
        RwLock,
    },
};

use crate::websocket::error::Error;
use serde_json::Value;
use tokio::sync::mpsc;

// RequestResult enum
#[derive(Debug, Clone)]
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

// WsconnMessage enum
#[derive(Debug)]
pub enum WsconnMessage {
    // call received from user and optional node index
    Message(Value, Option<usize>),
    Reconnect(),
}

impl From<WsconnMessage> for Value {
    fn from(msg: WsconnMessage) -> Self {
        match msg {
            WsconnMessage::Message(msg, _) => msg,
            WsconnMessage::Reconnect() => Value::Null,
        }
    }
}

// WsChannelErr enum
#[derive(Debug, Clone)]
pub enum WsChannelErr {
    Closed(usize),
}

#[derive(Debug, Clone)]
pub struct UserData {
    pub message_channel: mpsc::UnboundedSender<RequestResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeSubInfo {
    pub node_id: usize,
    pub subscription_id: String,
}

#[derive(Debug, Clone)]
pub struct IncomingResponse {
    pub content: Value,
    pub node_id: usize,
}

pub struct SubscriptionData {
    users: Arc<RwLock<HashMap<u32, UserData>>>,
    subscriptions: Arc<RwLock<HashMap<NodeSubInfo, HashSet<u32>>>>,
    incoming_subscriptions: Arc<RwLock<HashMap<String, NodeSubInfo>>>,
}

impl SubscriptionData {
    pub fn new() -> Self {
        SubscriptionData {
            users: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            incoming_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn add_user(&self, user_id: u32, user_data: UserData) {
        let mut users = self.users.write().unwrap();
        users.insert(user_id, user_data);
    }

    pub fn remove_user(&self, user_id: u32) {
        let mut users = self.users.write().unwrap();
        if users.remove(&user_id).is_some() {
            let mut subscriptions = self.subscriptions.write().unwrap();
            for user_subscriptions in subscriptions.values_mut() {
                user_subscriptions.remove(&user_id);
            }
        }
    }

    // Used to add a new subscription to the active subscription list
    pub fn register_subscription(
        &self,
        subscription: Value,
        subscription_id: String,
        node_id: usize,
    ) {
        // TODO: pepega
        let subscription = format!("{}", subscription["params"]);

        let mut incoming_subscriptions = self.incoming_subscriptions.write().unwrap();
        println!(
            "register_subscription inserting: {:?}",
            subscription.clone()
        );
        incoming_subscriptions.insert(
            subscription.clone(),
            NodeSubInfo {
                node_id,
                subscription_id,
            },
        );
        println!(
            "register_subscription: {:?}",
            incoming_subscriptions.get(&subscription)
        );
    }

    pub fn unregister_subscription(&self, subscription_request: String) {
        let mut incoming_subscriptions = self.incoming_subscriptions.write().unwrap();
        incoming_subscriptions.remove(&subscription_request);
    }

    // Subscribe user to existing subscription and return the subscription id
    //
    // If the subscription does not exist, return error
    pub fn subscribe_user(&self, user_id: u32, subscription: Value) -> Result<String, Error> {
        if subscription["params"].as_array().is_none()
            || subscription["params"].as_array().unwrap().is_empty()
        {
            return Err(format!("Invalid subscription params for {}", subscription).into());
        }

        // TODO: pepega
        let subscription = format!("{}", subscription["params"]);

        println!("subscribe_user finding: {}", subscription);
        let incoming_subscriptions = self.incoming_subscriptions.read().unwrap();
        let node_sub_info = match incoming_subscriptions.get(&subscription) {
            Some(rax) => rax,
            None => return Err(format!("Subscription {} does not exist!", subscription).into()),
        };

        let mut subscriptions = self.subscriptions.write().unwrap();
        subscriptions
            .entry(node_sub_info.clone())
            .or_default()
            .insert(user_id);

        Ok(node_sub_info.subscription_id.clone())
    }

    // Unsubscribe a user from a subscription
    pub fn unsubscribe_user(&self, user_id: u32, subscription_id: String) {
        let mut subscriptions = self.subscriptions.write().unwrap();
        let mut subscriptions_to_update = Vec::new();

        // Finding all subscriptions matching the subscription_id and user_id
        for (node_sub_info, subscribers) in subscriptions.iter() {
            if node_sub_info.subscription_id == subscription_id && subscribers.contains(&user_id) {
                subscriptions_to_update.push(node_sub_info.clone());
            }
        }

        // Unsubscribing the user from the found subscriptions
        for node_sub_info in subscriptions_to_update {
            if let Some(subscribers) = subscriptions.get_mut(&node_sub_info) {
                subscribers.remove(&user_id);
            }
        }
    }

    // Return the node_id for a given subscription_id
    pub fn get_node_from_id(&self, subscription_id: &str) -> Option<usize> {
        let incoming_subscriptions = self.incoming_subscriptions.read().unwrap();
        incoming_subscriptions
            .iter()
            .find_map(|(_, node_sub_info)| {
                if node_sub_info.subscription_id == subscription_id {
                    Some(node_sub_info.node_id)
                } else {
                    None
                }
            })
    }

    pub fn get_users_for_subscription(&self, subscription_id: &str) -> Vec<u32> {
        let subscriptions = self.subscriptions.read().unwrap();
        let mut users = Vec::new();

        for (node_sub_info, subscribers) in subscriptions.iter() {
            if node_sub_info.subscription_id == subscription_id {
                users.extend(subscribers.iter().copied());
                break;
            }
        }

        users
    }

    pub async fn dispatch_to_subscribers(
        &self,
        subscription_id: &str,
        node_id: usize,
        message: &RequestResult,
    ) -> Result<(), Error> {
        if let RequestResult::Call(_) = message {
            return Err("Trying to send a call as a subscription!".into());
        }

        let node_sub_info = NodeSubInfo {
            node_id,
            subscription_id: subscription_id.to_string(),
        };

        let users = self.users.read().unwrap();
        if let Some(subscribers) = self.subscriptions.read().unwrap().get(&node_sub_info) {
            if subscribers.is_empty() {
                self.unregister_subscription(subscription_id.to_string());
                println!(
                    "NO MORE USERS TO SEND THIS SUBSCRIPTION TO. ID: {}",
                    subscription_id
                );
            }
            for &user_id in subscribers {
                if let Some(user) = users.get(&user_id) {
                    user.message_channel
                        .send(message.clone())
                        .unwrap_or_else(|e| {
                            println!("Error sending message to user {}: {}", user_id, e);
                        });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::sync::mpsc;

    fn setup_user_and_subscription_data() -> (
        SubscriptionData,
        u32,
        mpsc::UnboundedReceiver<RequestResult>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let user_data = UserData {
            message_channel: tx,
        };
        let user_id = 100;
        let subscription_data = SubscriptionData::new();
        subscription_data.add_user(user_id, user_data);
        (subscription_data, user_id, rx)
    }

    #[tokio::test]
    async fn test_add_and_remove_user() {
        let (subscription_data, user_id, _) = setup_user_and_subscription_data();

        assert!(subscription_data
            .users
            .read()
            .unwrap()
            .contains_key(&user_id));
        subscription_data.remove_user(user_id);
        assert!(!subscription_data
            .users
            .read()
            .unwrap()
            .contains_key(&user_id));
    }

    #[tokio::test]
    async fn test_get_node_from_id() {
        let subscription_data = SubscriptionData::new();

        // Setup test data
        let node_id = 42;
        let subscription_id = "sub123".to_string();
        let subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});

        // Register a subscription
        subscription_data.register_subscription(
            subscription_request,
            subscription_id.clone(),
            node_id,
        );

        // Verify that get_node_from_id returns the correct node_id
        assert_eq!(
            subscription_data.get_node_from_id(&subscription_id),
            Some(node_id),
            "get_node_from_id should return the correct node_id"
        );

        // Verify for a non-existent subscription_id
        assert_eq!(
            subscription_data.get_node_from_id("nonexistent"),
            None,
            "get_node_from_id should return None for a non-existent subscription_id"
        );
    }

    #[tokio::test]
    async fn test_subscribe_and_unsubscribe_user() {
        let (subscription_data, user_id, _) = setup_user_and_subscription_data();
        let subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});
        let subscription_id = "200".to_string();
        let node_id = 1;

        subscription_data.register_subscription(
            subscription_request.clone(),
            subscription_id.clone(),
            node_id,
        );
        subscription_data
            .subscribe_user(user_id, subscription_request.clone())
            .unwrap();
        assert!(subscription_data
            .subscriptions
            .read()
            .unwrap()
            .iter()
            .any(|(k, v)| {
                k.node_id == node_id && k.subscription_id == subscription_id && v.contains(&user_id)
            }));

        subscription_data.unsubscribe_user(user_id, subscription_id.clone());
        assert!(!subscription_data
            .subscriptions
            .read()
            .unwrap()
            .iter()
            .any(|(k, v)| {
                k.node_id == node_id && k.subscription_id == subscription_id && v.contains(&user_id)
            }));
    }

    #[tokio::test]
    async fn test_dispatch_to_subscribers() {
        let (subscription_data, user_id, mut rx) = setup_user_and_subscription_data();
        let subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});
        let subscription_id = "300".to_string();
        let node_id = 1;
        let message =
            RequestResult::Subscription(serde_json::Value::String("test message".to_string()));

        subscription_data.register_subscription(
            subscription_request.clone(),
            subscription_id.clone(),
            node_id,
        );
        subscription_data
            .subscribe_user(user_id, subscription_request)
            .unwrap();
        subscription_data
            .dispatch_to_subscribers(&subscription_id, node_id, &message)
            .await
            .unwrap();

        match rx.recv().await {
            Some(RequestResult::Subscription(msg)) => assert_eq!(msg, "test message"),
            _ => panic!("Expected to receive a subscription message"),
        }
    }

    #[tokio::test]
    async fn test_remove_nonexistent_user() {
        let (subscription_data, _, _) = setup_user_and_subscription_data();
        let non_existent_user_id = 999;

        assert!(!subscription_data
            .users
            .read()
            .unwrap()
            .contains_key(&non_existent_user_id));
        subscription_data.remove_user(non_existent_user_id);
        assert!(!subscription_data
            .users
            .read()
            .unwrap()
            .contains_key(&non_existent_user_id));
    }

    #[tokio::test]
    async fn test_unsubscribe_nonexistent_subscription() {
        let (subscription_data, user_id, _) = setup_user_and_subscription_data();
        let nonexistent_subscription_id = "sub400".to_string();
        let nonexistent_node_id = 10000;

        let nonexistent_node_sub_info = NodeSubInfo {
            node_id: nonexistent_node_id,
            subscription_id: nonexistent_subscription_id.clone(),
        };

        subscription_data.unsubscribe_user(user_id, nonexistent_subscription_id.clone());
        assert!(subscription_data
            .subscriptions
            .read()
            .unwrap()
            .get(&nonexistent_node_sub_info)
            .is_none());
    }

    #[tokio::test]
    async fn test_dispatch_to_empty_subscription_list() {
        let subscription_data = SubscriptionData::new();
        let empty_subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});
        let empty_subscription_id = "500".to_string();
        let empty_node_id = 10000;
        let message = RequestResult::Subscription(serde_json::Value::String(
            "empty test message".to_string(),
        ));

        // No users are subscribed to this subscription
        subscription_data.register_subscription(
            empty_subscription_request,
            empty_subscription_id.clone(),
            empty_node_id,
        );
        let dispatch_result = subscription_data
            .dispatch_to_subscribers(&empty_subscription_id, empty_node_id, &message)
            .await;
        assert!(dispatch_result.is_ok()); // Should succeed even though there are no subscribers
    }

    #[tokio::test]
    async fn test_get_users_for_subscription() {
        let (subscription_data, user_id, _) = setup_user_and_subscription_data();
        let subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});
        let subscription_id = "200".to_string();
        let node_id = 1;

        // Register and subscribe a user to the subscription
        subscription_data.register_subscription(
            subscription_request.clone(),
            subscription_id.clone(),
            node_id,
        );
        subscription_data
            .subscribe_user(user_id, subscription_request.clone())
            .unwrap();

        // Test get_users_for_subscription function
        let users = subscription_data.get_users_for_subscription(&subscription_id);
        assert_eq!(users.len(), 1);
        assert!(users.contains(&user_id));

        // Test with a non-existent subscription_id
        let non_existent_subscription_id = "nonexistent".to_string();
        let empty_users =
            subscription_data.get_users_for_subscription(&non_existent_subscription_id);
        assert!(empty_users.is_empty());
    }

    #[tokio::test]
    async fn test_dispatch_to_nonexistent_subscription() {
        let subscription_data = SubscriptionData::new();
        let _nonexistent_subscription_request = "sub600".to_string();
        let nonexistent_subscription_id = "600".to_string();
        let nonexistent_node_id = 10000;

        let message = RequestResult::Subscription(serde_json::Value::String(
            "nonexistent subscription message".to_string(),
        ));

        let dispatch_result = subscription_data
            .dispatch_to_subscribers(&nonexistent_subscription_id, nonexistent_node_id, &message)
            .await;
        assert!(dispatch_result.is_ok()); // Should succeed as it should handle subscriptions with no users gracefully
    }
}
