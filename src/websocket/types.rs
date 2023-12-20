use std::{
    collections::{
        HashMap,
        HashSet,
    },
    sync::{
        Arc,
        RwLock,
    },
};

use crate::websocket::error::Error;

use serde_json::Value;
use tokio::sync::mpsc;

// Used to either specify if its an incoming call or a subscription
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

// Internal messages for the websocket manager
#[derive(Debug)]
pub enum WsconnMessage {
    Message(Value),
    Reconnect(),
}

impl From<WsconnMessage> for Value {
    fn from(msg: WsconnMessage) -> Self {
        match msg {
            WsconnMessage::Message(msg) => msg,
            WsconnMessage::Reconnect() => Value::Null,
        }
    }
}

// Errors to send to the health check when a WsConn fails
#[derive(Debug)]
pub enum WsChannelErr {
    Closed(usize),
}

pub struct UserData {
    pub message_channel: mpsc::UnboundedSender<RequestResult>,
}

pub struct SubscriptionData {
    pub users: Arc<RwLock<HashMap<u64, UserData>>>,
    // Mapping from subscription ID to a set of user IDs
    pub subscriptions: Arc<RwLock<HashMap<String, HashSet<u64>>>>,
}

impl SubscriptionData {
    pub fn new() -> Self {
        SubscriptionData {
            users: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Add or update a user
    pub fn add_user(&self, user_id: u64, user_data: UserData) {
        let mut users = self.users.write().unwrap();
        users.insert(user_id, user_data);
    }

    // Remove a user and clean up their subscriptions
    pub fn remove_user(&self, user_id: u64) {
        let mut users = self.users.write().unwrap();
        if users.remove(&user_id).is_some() {
            let mut subscriptions = self.subscriptions.write().unwrap();
            for user_subscriptions in subscriptions.values_mut() {
                user_subscriptions.remove(&user_id);
            }
        }
    }

    // Subscribe a user to a subscription
    pub fn subscribe_user(&self, user_id: u64, subscription_id: String) {
        let mut subscriptions = self.subscriptions.write().unwrap();
        subscriptions
            .entry(subscription_id)
            .or_default()
            .insert(user_id);
    }

    // Unsubscribe a user from a subscription
    pub fn unsubscribe_user(&self, user_id: u64, subscription_id: String) {
        if let Some(subscribers) = self
            .subscriptions
            .write()
            .unwrap()
            .get_mut(&subscription_id)
        {
            subscribers.remove(&user_id);
            // Check length, and if 0 send unsubscribe message to node
            // TODO: Implement the logic for this
            if subscribers.is_empty() {
                println!("NO MORE USERS TO SEND THIS SUBSCRIPTION TO. ID: {}", {
                    subscription_id
                })
            }
        }
    }

    // Dispatch a message to all users subscribed to a subscription
    pub async fn dispatch_to_subscribers(
        &self,
        subscription_id: &str,
        message: &RequestResult,
    ) -> Result<(), Error> {
        // TODO: We can remove this later
        match message {
            RequestResult::Call(_) => return Err("Trying to send a call as a subscription!".into()),
            RequestResult::Subscription(_) => {}
        };

        let users = self.users.read().unwrap();
        if let Some(subscribers) = self.subscriptions.read().unwrap().get(subscription_id) {
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
    use tokio::sync::mpsc;

    fn setup_user_and_subscription_data() -> (
        SubscriptionData,
        u64,
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
    async fn test_subscribe_and_unsubscribe_user() {
        let (subscription_data, user_id, _) = setup_user_and_subscription_data();
        let subscription_id = "200".to_string();

        subscription_data.subscribe_user(user_id, subscription_id.clone());
        assert!(subscription_data
            .subscriptions
            .read()
            .unwrap()
            .get(&subscription_id)
            .unwrap()
            .contains(&user_id));

        subscription_data.unsubscribe_user(user_id, subscription_id.clone());
        assert!(!subscription_data
            .subscriptions
            .read()
            .unwrap()
            .get(&subscription_id)
            .unwrap()
            .contains(&user_id));
    }

    #[tokio::test]
    async fn test_dispatch_to_subscribers() {
        let (subscription_data, user_id, mut rx) = setup_user_and_subscription_data();
        let subscription_id = 300.to_string();
        let message =
            RequestResult::Subscription(serde_json::Value::String("test message".to_string()));

        subscription_data.subscribe_user(user_id, subscription_id.clone());
        subscription_data
            .dispatch_to_subscribers(&subscription_id, &message)
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
        let nonexistent_subscription_id = 400.to_string();

        subscription_data.unsubscribe_user(user_id, nonexistent_subscription_id.clone());
        assert!(subscription_data
            .subscriptions
            .read()
            .unwrap()
            .get(&nonexistent_subscription_id)
            .is_none());
    }

    #[tokio::test]
    async fn test_dispatch_to_empty_subscription_list() {
        let subscription_data = SubscriptionData::new();
        let empty_subscription_id = 500.to_string();
        let message = RequestResult::Subscription(serde_json::Value::String(
            "empty test message".to_string(),
        ));

        // No users are subscribed to this subscription
        let dispatch_result = subscription_data
            .dispatch_to_subscribers(&empty_subscription_id, &message)
            .await;
        assert!(dispatch_result.is_ok()); // Should succeed even though there are no subscribers
    }

    #[tokio::test]
    async fn test_dispatch_to_nonexistent_subscription() {
        let subscription_data = SubscriptionData::new();
        let nonexistent_subscription_id = 600.to_string();
        let message = RequestResult::Subscription(serde_json::Value::String(
            "nonexistent subscription message".to_string(),
        ));

        let dispatch_result = subscription_data
            .dispatch_to_subscribers(&nonexistent_subscription_id, &message)
            .await;
        assert!(dispatch_result.is_ok()); // Should succeed as it should handle subscriptions with no users gracefully
    }
}
