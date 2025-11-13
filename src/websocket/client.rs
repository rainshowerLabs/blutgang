use crate::{
    balancer::{
        format::replace_block_tags,
        processing::{
            cache_query,
            update_rpc_latency,
            CacheArgs,
        },
        selection::select::pick,
    },
    database::types::GenericBytes,
    db_get,
    rpc::{
        method::EthRpcMethod,
        types::Rpc,
    },
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

/// Accepts incoming internal WS messages.
///
/// Upon receiving a `WsconnMessage::Reconnect()` it will drop all current WS
/// connections and initiate new ones from the `rpc_list`.
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    ws_handles: Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<WsconnMessage>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
) {
    // Initialize WebSocket connections
    update_ws_connections(&rpc_list, &ws_handles, &broadcast_tx, &ws_error_tx).await;

    // Buffer for WS subscriptions when all nodes are ded
    let mut ws_buffer: Vec<Value> = Vec::new();

    while let Some(message) = incoming_rx.recv().await {
        match message {
            WsconnMessage::Message(incoming, specified_index) => {
                handle_incoming_message(
                    &ws_handles,
                    &rpc_list,
                    incoming,
                    specified_index,
                    &mut ws_buffer,
                )
                .await;
            }
            WsconnMessage::Reconnect() => {
                update_ws_connections(&rpc_list, &ws_handles, &broadcast_tx, &ws_error_tx).await;
                unload_buffer(&rpc_list, &ws_handles, &mut ws_buffer).await;
            }
        }
    }
}

/// Updates the active WS handles to match the active connections.
async fn update_ws_connections(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ws_handles: &Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    broadcast_tx: &broadcast::Sender<IncomingResponse>,
    ws_error_tx: &mpsc::UnboundedSender<WsChannelErr>,
) {
    let ws_vec = create_ws_vec(rpc_list, broadcast_tx, ws_error_tx).await;
    let mut ws_handle_guard = ws_handles.write().unwrap_or_else(|e| {
        // Handle the case where the ws_handles RwLock is poisoned
        tracing::error!(?e);
        e.into_inner()
    });
    *ws_handle_guard = ws_vec;
}

/// Dispatches buffered WS subscriptions out to nodes.
async fn unload_buffer(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ws_handles: &Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    ws_buffer: &mut Vec<Value>,
) {
    for i in 0..ws_buffer.len() {
        let incoming = ws_buffer[i].clone();
        handle_incoming_message(ws_handles, rpc_list, incoming, None, ws_buffer).await;
    }
    ws_buffer.clear();
}

/// Sends an incoming request to a WS connection.
///
/// Indexes can be specified via the `specified_index` param.
async fn handle_incoming_message(
    ws_handles: &Arc<RwLock<Vec<Option<mpsc::UnboundedSender<Value>>>>>,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    incoming: Value,
    specified_index: Option<usize>,
    ws_buffer: &mut Vec<Value>,
) {
    let rpc_position = if let Some(index) = specified_index {
        index
    } else {
        let mut rpc_list_guard = rpc_list.write().unwrap_or_else(|e| {
            // Handle the case where the rpc_list RwLock is poisoned
            tracing::error!(?e);
            e.into_inner()
        });

        match pick(&mut rpc_list_guard).1 {
            Some(position) => position,
            None => {
                // Check if the incoming content is a subscription.
                //
                // We do this because we want to send it to a buffer
                // in case we have no available RPCs.
                let method = &incoming["method"];
                if method.eq(&EthRpcMethod::Subscription) || method.eq(&EthRpcMethod::Subscribe) {
                    ws_buffer.push(incoming);
                }
                tracing::error!("No RPC position available");
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
            tracing::error!("ws_conn_manager error: failed to send message");
        }
    } else {
        tracing::error!(rpc_position, "No WS connection at index");
    }
}

/// Creates new WS connections off of RPCs in `rpc_list`.
///
/// Returns a Vec of channels that can be used to send values
/// to different individual WS connections.
pub async fn create_ws_vec(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    broadcast_tx: &broadcast::Sender<IncomingResponse>,
    ws_error_tx: &mpsc::UnboundedSender<WsChannelErr>,
) -> Vec<Option<mpsc::UnboundedSender<Value>>> {
    let rpc_list_clone = rpc_list
        .read()
        .unwrap_or_else(|e| {
            // Handle the case where the rpc_list RwLock is poisoned
            tracing::error!(?e);
            e.into_inner()
        })
        .clone();
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

/// Represents a single WS connection to an RPC.
///
/// Accepts incoming requests via `incoming_rx` and send responses
/// via `broadcast_tx`. Messages are *discovered* by their respective
/// senders via the `"id"` field.
///
/// In case of an error where the connection is forced to close,
/// a message will be sent via the `ws_error_tx` channel alerting
/// the health check module.
pub async fn ws_conn(
    rpc: Rpc,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
    index: usize,
) {
    let ws_stream = match connect_async(&rpc.ws_url.unwrap()).await {
        Ok((ws_stream, _)) => ws_stream,
        Err(_) => {
            tracing::error!(
                "Node {} dropped their connection in the middle of WS init!",
                rpc.name
            );
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Thread for sending messages
    let sender_error_tx = ws_error_tx.clone();
    tokio::spawn(async move {
        while let Some(incoming) = incoming_rx.recv().await {
            tracing::debug!("ws_conn[{}], send: {:?}", index, incoming);

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
                    tracing::debug!("ws_conn[{}], recv: {:?}", index, message);

                    let mut ws_message = match message.into_text() {
                        Ok(rax) => rax,
                        Err(e) => {
                            tracing::error!(?e, "Received malformed message from ws_conn");
                            let _ = ws_error_tx.send(WsChannelErr::Closed(index));
                            break;
                        }
                    };

                    let rax = match unsafe { from_str(&mut ws_message) } {
                        Ok(rax) => rax,
                        Err(_e) => {
                            {
                                tracing::warn!(?_e, "Couldn't deserialize ws_conn response");
                            }

                            continue;
                        }
                    };

                    let incoming = IncomingResponse {
                        node_id: index,
                        content: rax,
                    };

                    let _ = broadcast_tx.send(incoming);
                    let time = time.elapsed();
                    update_rpc_latency(&rpc_list, index, time);
                    tracing::info!(?time, "WS request time");
                }
                Err(_) => {
                    let _ = ws_error_tx.send(WsChannelErr::Closed(index));
                    break;
                }
            }
        }
    });
}

/// Processes an individual RPC request received via WebSockets.
///
/// Contains logic for retreiving from cache, sending to the internal
/// WS pipeline, retreiving and returning received responses.
pub async fn execute_ws_call<K, V>(
    mut call: Value,
    user_id: u32,
    incoming_tx: &mpsc::UnboundedSender<WsconnMessage>,
    broadcast_rx: broadcast::Receiver<IncomingResponse>,
    sub_data: &Arc<SubscriptionData>,
    cache_args: &CacheArgs<K, V>,
) -> Result<String, WsError>
where
    K: GenericBytes + From<[u8; 32]>,
    V: GenericBytes + From<Vec<u8>>,
{
    tracing::debug!(
        "Received incoming WS call from user_id {}: {:?}",
        user_id,
        call
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

    if let Ok(Some(mut rax)) = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned().into()) {
        let mut cached: Value = from_slice(rax.as_mut()).unwrap();
        cached["id"] = id;
        return Ok(cached.to_string());
    }

    // Remove and unsubscribe user is "eth_unsubscribe"
    if call["method"].eq(&EthRpcMethod::Unsubscribe) {
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
        tracing::info!("execute_ws_call: index: {index:?}");

        sub_data.unsubscribe_user(user_id, subscription_id);

        return Ok(format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":true}}",
            id
        ));
    }

    let is_subscription = call["method"].eq(&EthRpcMethod::Subscribe);
    if is_subscription {
        // Check if we're already subscribed to this
        // if so return the subscription id and add this user to the dispatch
        // if not continue
        if let Ok(rax) = sub_data.subscribe_user(user_id, call.clone()) {
            tracing::debug!("has subscription already");
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
        tracing::debug!("is subscription!");
        tracing::debug!(?response.content, "response content");
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

        tracing::info!(sub_id, "sub_id");
        sub_data.register_subscription(call.clone(), sub_id.clone(), response.node_id);
        sub_data.subscribe_user(user_id, call)?;
    } else {
        cache_query(&mut response.content.to_string(), call, tx_hash, cache_args).await;
    }

    response.content["id"] = id;
    Ok(response.content.to_string())
}

/// Listens for a respond corresponding to our internal `user_id`.
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
    use crate::rpc::method::EthRpcMethod;

    use super::*;
    use serde_json::json;
    use std::time::Duration;

    // Helper function to create a mock Rpc object
    fn mock_rpc(url: &str) -> Rpc {
        Rpc::new(
            format!("http://{}", url).parse().unwrap(),
            Some(format!("ws://{}", url).parse().unwrap()),
            10000,
            1,
            10.0,
        )
    }

    async fn create_mock_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        let rpc_list = Arc::new(RwLock::new(vec![
            Rpc::new(
                "http://test1".parse().unwrap(),
                Some("ws://test1".parse().unwrap()),
                0,
                0,
                0.0,
            ),
            Rpc::new(
                "http://test2".parse().unwrap(),
                Some("ws://test2".parse().unwrap()),
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
        let mut ws_buffer: Vec<Value> = Vec::new();

        handle_incoming_message(
            &ws_handles,
            &rpc_list,
            incoming.clone(),
            Some(0),
            &mut ws_buffer,
        )
        .await;

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
            "method": EthRpcMethod::Subscribe,
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
            "method": EthRpcMethod::BlockNumber,
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
