use crate::{
    config::setup::WS_SUB_MANAGER_ID,
    websocket::{
        error::Error,
        types::{
            IncomingResponse,
            RequestResult,
            SubscriptionData,
            WsconnMessage,
        },
    },
};

use std::{
    collections::HashMap,
    sync::Arc,
};

use tokio::sync::{
    broadcast,
    mpsc,
};

use serde_json::json;

// Sends all subscriptions to their relevant nodes
pub fn subscription_dispatcher(
    mut rx: broadcast::Receiver<IncomingResponse>,
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
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
            // this is retarded???
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
                // Getting true means that we should unsubscribe from the subscription
                // as thre are no more users needing it.
                Ok(true) => {
                    let unsub = json!({"jsonrpc": "2.0","id": WS_SUB_MANAGER_ID,"method": "eth_unsubscribe","params": [id]});
                    let message = WsconnMessage::Message(unsub, Some(response.node_id));
                    let _ = incoming_tx.send(message);
                }
                // False means tht we do not need to do anything
                Ok(false) => {}
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

// Moves all subscriptions from one node to another one.
// Used during node failiure. Do not use this liberally as it is very heavy.
pub async fn move_subscriptions(
    incoming_tx: &mpsc::UnboundedSender<WsconnMessage>,
    mut rx: broadcast::Receiver<IncomingResponse>,
    sub_data: &Arc<SubscriptionData>,
    node_id: usize,
) -> Result<(), Error> {
    // Collect all subscriptions/ids we have assigned to `node_id` and put them in a vec
    let subs = sub_data.get_subscription_by_node(node_id);
    let ids = sub_data.get_sub_id_by_node(node_id);

    // We want to send unsubscribe messages (for postoriety) to node_id
    for id in ids {
        let unsub = json!({"jsonrpc": "2.0","id": WS_SUB_MANAGER_ID,"method": "eth_unsubscribe","params": [id]});
        let message = WsconnMessage::Message(unsub, Some(node_id));
        let _ = incoming_tx.send(message);
    }

    // We want to send subscription messages to `target`, register them, and move over the users
    let _ = rx; // bind `rx` so we have time to process all messages
    let mut pairs: HashMap<u32, String> = HashMap::new();
    let mut id = WS_SUB_MANAGER_ID + 32000; // TODO: replace with magic #
    for params in subs {
        id += 1;
        let sub = json!({"jsonrpc": "2.0","id": id,"method": "eth_subscribe","params": vec![params.clone()]});
        let message = WsconnMessage::Message(sub, None);

        pairs.insert(id, params);

        let _ = incoming_tx.send(message);
    }

    // Listen on `rx` for incoming messages.
    // We're only interested in ones that have the right ID as specified in pairs
    while !pairs.is_empty() {
        // TODO: errors!
        let response = rx.recv().await?;

        // Discard any response that does not have a proper ID
        let pair_id = match response.content["id"].as_u64() {
            Some(rax) => rax as u32,
            None => return Err(Error::InvalidData("No ID in response!".to_string())),
        };

        let params = match pairs.get(&pair_id) {
            Some(rax) => rax.to_string(),
            None => continue,
        };

        let sub_id = match sub_data.get_sub_id_by_params(&params) {
            Some(rax) => rax,
            None => return Err(Error::MissingSubscription()),
        };
        match sub_data.move_subscriptions(response.node_id, params, sub_id) {
            Ok(_) => {}
            Err(err) => return Err(err),
        };

        pairs.remove(&pair_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{
        broadcast,
        mpsc,
    };

    #[tokio::test]
    async fn test_subscription_dispatcher() {
        let (tx, rx) = broadcast::channel(10);
        let (incoming_tx, _incoming_rx) = mpsc::unbounded_channel();
        let sub_data = Arc::new(SubscriptionData::new());
        let user_id = 1;
        let subscription_id = "sub123";

        // Mock user and subscription setup
        let (user_tx, mut user_rx) = mpsc::unbounded_channel();
        sub_data.add_user(user_id, user_tx);

        let subscription_request =
            json!({"jsonrpc":"2.0","id": 1, "method": "eth_subscribe", "params": ["newHeads"]});
        sub_data.register_subscription(
            subscription_request.clone(),
            subscription_id.to_string(),
            0,
        );
        sub_data
            .subscribe_user(user_id, subscription_request)
            .unwrap();

        subscription_dispatcher(rx, incoming_tx, Arc::clone(&sub_data));

        let subscription_content =
            json!({"method": "eth_subscription", "params": {"subscription": subscription_id}});
        let incoming_response = IncomingResponse {
            content: subscription_content,
            node_id: 0,
        };
        tx.send(incoming_response).unwrap();

        // Check if the user receives the message
        if let Some(RequestResult::Subscription(msg)) = user_rx.recv().await {
            assert_eq!(
                msg,
                json!({"method": "eth_subscription", "params": {"subscription": subscription_id}})
            );
        } else {
            panic!("User did not receive the expected message.");
        }
    }

    #[tokio::test]
    async fn test_move_subscriptions() {
        let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel();
        let (tx, rx) = broadcast::channel(10);
        let sub_data = Arc::new(SubscriptionData::new());
        let node_id = 1;
        let user_id = 2;

        // Setup for subscriptions
        let subscription_request =
            json!({"jsonrpc":"2.0","id": 2, "method": "eth_subscribe", "params": ["newHeads"]});
        sub_data.register_subscription(subscription_request.clone(), "sub789".to_string(), node_id);
        sub_data
            .subscribe_user(user_id, subscription_request)
            .unwrap();

        // Spawn a thread to handle incoming subscription requests
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            while let Some(WsconnMessage::Message(message, _)) = incoming_rx.recv().await {
                if message["method"] == "eth_subscribe" {
                    let id = message["id"].as_u64().unwrap() as u32;
                    let random_result = rand::thread_rng().gen::<u64>().to_string();
                    let mock_response = IncomingResponse {
                        content: json!({"jsonrpc": "2.0", "id": id, "result": random_result}),
                        node_id: 2, // new node ID
                    };
                    tokio::time::sleep(Duration::from_millis(50)).await; // Simulate network delay
                    tx_clone.send(mock_response).unwrap();
                }
            }
        });

        // Execute move_subscriptions
        let move_result = move_subscriptions(&incoming_tx, rx, &Arc::clone(&sub_data), node_id).await;
        assert!(move_result.is_ok(), "move_subscriptions should succeed");

        // Verify the mock responses have been processed and subscriptions moved
        let og_subs = sub_data.get_subscription_by_node(1);
        assert!(
            og_subs.is_empty(),
            "Subscriptions should have been moved to the new node"
        );

        let moved_subs = sub_data.get_subscription_by_node(2); // new node ID
        assert!(
            !moved_subs.is_empty(),
            "Subscriptions should have been moved to the new node"
        );
    }
}
