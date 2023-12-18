use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::mpsc;

use std::sync::{Arc, RwLock};
use std::collections::{HashMap, HashSet};
use futures::SinkExt;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

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
    pub subscriptions: Arc<RwLock<HashMap<u64, HashSet<u64>>>>,
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
    pub fn subscribe_user(&self, user_id: u64, subscription_id: u64) {
        let mut subscriptions = self.subscriptions.write().unwrap();
        subscriptions.entry(subscription_id).or_default().insert(user_id);
    }

    // Unsubscribe a user from a subscription
    pub fn unsubscribe_user(&self, user_id: u64, subscription_id: u64) {
        if let Some(subscribers) = self.subscriptions.write().unwrap().get_mut(&subscription_id) {
            subscribers.remove(&user_id);
        }
    }

    // Dispatch a message to all users subscribed to a subscription
    pub async fn dispatch_to_subscribers(&self, subscription_id: u64, message: &RequestResult) -> Result<(), Error> {
        // TODO: We can remove this later
        match message {
            RequestResult::Call(_) => println!("\x1b[31mErr:\x1b[0m Trying to send Call as subscription!!!"),
            &RequestResult::Subscription(_) => {},
        }

        let users = self.users.read().unwrap();
        if let Some(subscribers) = self.subscriptions.read().unwrap().get(&subscription_id) {
            for &user_id in subscribers {
                if let Some(user) = users.get(&user_id) {
                    user.message_channel.send(message.clone()).unwrap_or_else(|e| {
                        println!("Error sending message to user {}: {}", user_id, e);
                    });
                }
            }
        }
        Ok(())
    }

}

