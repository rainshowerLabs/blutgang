use crate::{
    balancer::{
        format::replace_block_tags,
        processing::{
            cache_querry,
            update_rpc_latency,
            CacheArgs,
        },
        selection::select::pick,
    },
    log_err,
    log_info,
    rpc::types::Rpc,
    websocket::{
        error::WsError,
        types::{
            IncomingResponse,
            SubscriptionData,
            WsChannelErr,
            WsconnMessage,
        },
    },
};

use std::{
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
};

use futures_util::{
    SinkExt,
    StreamExt,
};
use serde_json::Value;
use simd_json::{
    from_slice,
    from_str,
};

use tokio::sync::{
    broadcast,
    mpsc,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message,
};

#[cfg(not(feature = "xxhash"))]
use blake3::hash;

#[cfg(feature = "xxhash")]
use xxhash_rust::xxh3::xxh3_64;

pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    ws_handles: Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<WsconnMessage>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
) {
    // Initialize WebSocket connections
    update_ws_connections(&rpc_list, &ws_handles, &broadcast_tx, &ws_error_tx).await;

    while let Some(message) = incoming_rx.recv().await {
        match message {
            WsconnMessage::Message(incoming, specified_index) => {
                handle_incoming_message(&ws_handles, &rpc_list, incoming, specified_index).await;
            }
            WsconnMessage::Reconnect() => {
                update_ws_connections(&rpc_list, &ws_handles, &broadcast_tx, &ws_error_tx).await;
            }
        }
    }
}

async fn update_ws_connections(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ws_handles: &Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    broadcast_tx: &broadcast::Sender<IncomingResponse>,
    ws_error_tx: &mpsc::UnboundedSender<WsChannelErr>,
) {
    let ws_vec = create_ws_vec(rpc_list, broadcast_tx, ws_error_tx).await;
    let mut ws_handle_guard = ws_handles.write().unwrap();
    *ws_handle_guard = ws_vec;
}

async fn handle_incoming_message(
    ws_handles: &Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    incoming: Value,
    specified_index: Option<usize>,
) {
    let rpc_position = if let Some(index) = specified_index {
        index
    } else {
        match pick(&mut rpc_list.write().unwrap()).1 {
            Some(position) => position,
            None => {
                log_err!("No RPC position available");
                return;
            }
        }
    };

    if let Some(ws) = ws_handles
        .read()
        .unwrap()
        .get(rpc_position)
        .and_then(|handle| handle.as_ref())
    {
        if ws.send(incoming).is_err() {
            log_err!("ws_conn_manager error: failed to send message");
        }
    } else {
        log_err!("No WS connection at index {}", rpc_position);
    }
}

pub async fn create_ws_vec(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    broadcast_tx: &broadcast::Sender<IncomingResponse>,
    ws_error_tx: &mpsc::UnboundedSender<WsChannelErr>,
) -> Vec<Option<mpsc::UnboundedSender<Value>>> {
    let rpc_list_clone = rpc_list.read().unwrap().clone();
    let mut ws_handles = Vec::new();

    for (index, rpc) in rpc_list_clone.iter().enumerate() {
        let (ws_conn_incoming_tx, ws_conn_incoming_rx) = mpsc::unbounded_channel();
        ws_handles.push(Some(ws_conn_incoming_tx));
        ws_conn(
            rpc.clone(),
            rpc_list.clone(),
            ws_conn_incoming_rx,
            broadcast_tx.clone(),
            ws_error_tx.clone(),
            index,
        )
        .await;
    }

    ws_handles
}

pub async fn ws_conn(
    rpc: Rpc,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
    index: usize,
) {
    let (ws_stream, _) = connect_async(&rpc.ws_url.unwrap())
        .await
        .expect("Failed to connect to WS");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Thread for sending messages
    let sender_error_tx = ws_error_tx.clone();
    tokio::spawn(async move {
        while let Some(incoming) = incoming_rx.recv().await {
            #[cfg(feature = "debug-verbose")]
            println!("ws_conn[{}], send: {:?}", index, incoming);

            if ws_sender
                .send(Message::Text(incoming.to_string()))
                .await
                .is_err()
            {
                let _ = sender_error_tx.send(WsChannelErr::Closed(index));
                break;
            }
        }
    });

    // Thread for receiving messages
    tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(message) => {
                    let time = Instant::now();
                    #[cfg(feature = "debug-verbose")]
                    println!("ws_conn[{}], recv: {:?}", index, message);

                    let rax = unsafe { from_str(&mut message.into_text().unwrap()).unwrap() };

                    let incoming = IncomingResponse {
                        node_id: index,
                        content: rax,
                    };

                    let _ = broadcast_tx.send(incoming);
                    let time = time.elapsed();
                    update_rpc_latency(&rpc_list, index, time);
                    log_info!("WS request time: {:?}", time);
                }
                Err(_) => {
                    let _ = ws_error_tx.send(WsChannelErr::Closed(index));
                    break;
                }
            }
        }
    });
}

pub async fn execute_ws_call(
    mut call: Value,
    user_id: u32,
    incoming_tx: &mpsc::UnboundedSender<WsconnMessage>,
    broadcast_rx: broadcast::Receiver<IncomingResponse>,
    sub_data: &Arc<SubscriptionData>,
    cache_args: &CacheArgs,
) -> Result<String, WsError> {
    #[cfg(feature = "debug-verbose")]
    println!(
        "Received incoming WS call from user_id {}: {:?}",
        user_id, call
    );

    let id = call["id"].take();
    let tx_hash = {
        #[cfg(not(feature = "xxhash"))]
        {
            hash(call.to_string().as_bytes())
        }
        #[cfg(feature = "xxhash")]
        {
            xxh3_64(call.to_string().as_bytes())
        }
    };

    if let Ok(Some(mut rax)) = cache_args.cache.get(tx_hash.as_bytes()) {
        let mut cached: Value = from_slice(&mut rax).unwrap();
        cached["id"] = id;
        return Ok(cached.to_string());
    }

    // Remove and unsubscribe user is "eth_unsubscribe"
    if call["method"] == "eth_unsubscribe" {
        // subscription_id is ["params"][0]
        let subscription_id = match call["params"][0].as_str() {
            Some(subscription_id) => subscription_id.to_string(),
            None => {
                return Ok(format!(
                    "{{\"jsonrpc\":\"2.0\", \"id\":{}, \"error\": \"Bad Subscription ID!\"}}",
                    id
                ));
            }
        };
        // we have to get the id of the subsctiption and what node is subscribed and send the message
        let index = match sub_data.get_node_from_id(&subscription_id) {
            Some(rax) => Some(rax),
            None => {
                return Ok(format!(
                    "{{\"jsonrpc\":\"2.0\", \"id\":{}, \"error\": \"false\"}}",
                    id
                ));
            }
        };
        println!("execute_ws_call: index: {:?}", index);

        sub_data.unsubscribe_user(user_id, subscription_id);

        return Ok(format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":true}}",
            id
        ));
    }

    let is_subscription = call["method"] == "eth_subscribe";
    if is_subscription {
        // Check if we're already subscribed to this
        // if so return the subscription id and add this user to the dispatch
        // if not continue
        if let Ok(rax) = sub_data.subscribe_user(user_id, call.clone()) {
            println!("has subscription already");
            return Ok(format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":\"{}\"}}",
                id, rax
            ));
        }
    } else {
        // Replace block tags if applicable
        call = replace_block_tags(&mut call, &cache_args.named_numbers);
    }

    call["id"] = user_id.into();
    incoming_tx.send(WsconnMessage::Message(call.clone(), None))?;
    let mut response = listen_for_response(user_id, broadcast_rx).await?;

    if is_subscription {
        #[cfg(feature = "debug-verbose")]
        println!("is subscription!");
        #[cfg(feature = "debug-verbose")]
        println!("response content: {:?}", response.content);
        // add the subscription id and add this user to the dispatch
        let sub_id = match response.content["result"].as_str() {
            Some(sub_id) => sub_id.to_string(),
            None => {
                return Ok(format!(
                    "\"jsonrpc\":\"2.0\", \"id\":{}, \"error\": \"Bad Subscription ID!\"",
                    id
                ));
            }
        };

        println!("\x1b[35mInfo:\x1b[0m sub_id: {}", sub_id);
        sub_data.register_subscription(call.clone(), sub_id.clone(), response.node_id);
        sub_data.subscribe_user(user_id, call)?;
    } else {
        cache_querry(&mut response.content.to_string(), call, tx_hash, cache_args);
    }

    response.content["id"] = id;
    Ok(response.content.to_string())
}

async fn listen_for_response(
    user_id: u32,
    mut broadcast_rx: broadcast::Receiver<IncomingResponse>,
) -> Result<IncomingResponse, WsError> {
    while let Ok(response) = broadcast_rx.recv().await {
        if response.content["id"].as_u64().unwrap_or(u32::MAX.into()) as u32 == user_id {
            return Ok(response);
        }
    }
    Err(WsError::NoWsResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    // Helper function to create a mock Rpc object
    fn mock_rpc(url: &str) -> Rpc {
        Rpc::new(
            format!("http://{}", url),
            Some(format!("ws://{}", url)),
            10000,
            1,
            10.0,
        )
    }

    async fn create_mock_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        let rpc_list = Arc::new(RwLock::new(vec![
            Rpc::new(
                "http://test1".to_string(),
                Some("ws://test1".to_string()),
                0,
                0,
                0.0,
            ),
            Rpc::new(
                "http://test2".to_string(),
                Some("ws://test2".to_string()),
                0,
                0,
                0.0,
            ),
        ]));
        rpc_list
    }

    // Helper function to setup the environment for ws_conn_manager tests
    fn setup_ws_conn_manager_test() -> (
        Arc<RwLock<Vec<Rpc>>>,
        mpsc::UnboundedSender<WsconnMessage>,
        mpsc::UnboundedReceiver<WsconnMessage>,
        broadcast::Sender<IncomingResponse>,
        mpsc::UnboundedSender<WsChannelErr>,
    ) {
        let rpc_list = Arc::new(RwLock::new(vec![
            mock_rpc("node1.example.com"),
            mock_rpc("node2.example.com"),
        ]));
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, _) = broadcast::channel(10);
        let (ws_error_tx, _) = mpsc::unbounded_channel();

        (
            rpc_list,
            incoming_tx,
            incoming_rx,
            broadcast_tx,
            ws_error_tx,
        )
    }

    #[tokio::test]
    async fn test_handle_incoming_message() {
        let rpc_list = create_mock_rpc_list().await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let ws_handles = Arc::new(RwLock::new(vec![Some(tx)]));
        let incoming = json!({"type": "test"});

        handle_incoming_message(&ws_handles, &rpc_list, incoming.clone(), Some(0)).await;

        // Check if the message was sent through the channel
        let received = rx.recv().await;
        assert_eq!(received, Some(incoming));
    }

    #[tokio::test]
    async fn test_ws_conn_handling_error() {
        let (_rpc_list, incoming_tx, mut incoming_rx, _broadcast_tx, _ws_error_tx) =
            setup_ws_conn_manager_test();

        // Sending a message that should cause an error
        let invalid_message = json!({"invalid": "message"});
        incoming_tx
            .send(WsconnMessage::Message(invalid_message, None))
            .unwrap();

        // Expecting an error response
        if let Some(WsconnMessage::Message(_, _)) = incoming_rx.recv().await {
            // Handling error cases here
        }
    }

    #[tokio::test]
    async fn test_execute_ws_subscription_and_call() {
        //
        // Test subscriptions
        //

        let (incoming_tx, _incoming_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(10);
        let sub_data = Arc::new(SubscriptionData::new());
        let cache_args = CacheArgs::default();

        let call = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_subscribe",
            "params": ["newHeads"]
        });

        // Simulate a response
        let b_clone = broadcast_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let response = IncomingResponse {
                content: json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1a2b3c"
                }),
                node_id: 0,
            };
            b_clone.send(response).unwrap();
        });

        let result = execute_ws_call(
            call,
            1,
            &incoming_tx,
            broadcast_rx.resubscribe(),
            &sub_data,
            &cache_args,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "{\"id\":1,\"jsonrpc\":\"2.0\",\"result\":\"0x1a2b3c\"}"
        );

        //
        // Test calls
        //

        let call = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_blockNumber"
        });

        // Simulate a response
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let response = IncomingResponse {
                content: json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1a2b3c"
                }),
                node_id: 0,
            };
            broadcast_tx.send(response).unwrap();
        });

        let result =
            execute_ws_call(call, 1, &incoming_tx, broadcast_rx, &sub_data, &cache_args).await;

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "{\"id\":1,\"jsonrpc\":\"2.0\",\"result\":\"0x1a2b3c\"}"
        );
    }

    #[tokio::test]
    async fn test_listen_for_response() {
        let (broadcast_tx, broadcast_rx) = broadcast::channel(10);

        // Simulate a response
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let response = IncomingResponse {
                content: json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": "0x1a2b3c"
                }),
                node_id: 0,
            };
            broadcast_tx.send(response).unwrap();
        });

        let result = listen_for_response(1, broadcast_rx).await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().content,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": "0x1a2b3c"
            })
        );
    }
}
