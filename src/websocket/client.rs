use crate::{
    balancer::{
        format::replace_block_tags,
        processing::{
            cache_query,
            update_rpc_latency,
            CacheArgs,
        },
        selection::{
            cache_rules::has_block_tag,
            select::pick,
        },
    },
    database::types::GenericBytes,
    db_get,
    invalid_request_body,
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
    collections::HashMap,
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

use blake3::hash;

/// Accepts incoming internal WS messages.
///
/// Upon receiving a `WsconnMessage::Reconnect()` it will drop all current WS
/// connections and initiate new ones from the `rpc_list`.
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<WsconnMessage>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
) {
    let mut connections = WsConnections {
        rpc_list,
        handles: HashMap::new(),
        broadcast_tx,
        ws_error_tx,
    };

    // Initialize WebSocket connections
    connections.reconnect().await;

    // Buffer for WS subscriptions when all nodes are ded
    let mut ws_buffer: Vec<Value> = Vec::new();

    while let Some(message) = incoming_rx.recv().await {
        match message {
            WsconnMessage::Message(incoming, specified_node) => {
                connections
                    .send(incoming, specified_node, &mut ws_buffer)
                    .await;
            }
            WsconnMessage::Reconnect() => {
                connections.reconnect().await;
                unload_buffer(&mut connections, &mut ws_buffer).await;
            }
        }
    }
}

/// The WS connections to the RPCs, and what is needed to open new ones.
struct WsConnections {
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    /// Channels to each RPC's connection, by [`Rpc::id`].
    ///
    /// Keyed by id rather than position: the health check moves RPCs in and
    /// out of `rpc_list` without reconnecting.
    handles: HashMap<usize, mpsc::UnboundedSender<Value>>,
    broadcast_tx: broadcast::Sender<IncomingResponse>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
}

impl WsConnections {
    /// Replaces all connections with new ones to the RPCs currently in `rpc_list`.
    async fn reconnect(&mut self) {
        let rpc_list_clone = self
            .rpc_list
            .read()
            .unwrap_or_else(|e| {
                // Handle the case where the rpc_list RwLock is poisoned
                tracing::error!(?e);
                e.into_inner()
            })
            .clone();

        let mut handles = HashMap::new();
        for rpc in rpc_list_clone.iter() {
            handles.insert(rpc.id(), self.open(rpc).await);
        }
        self.handles = handles;
    }

    /// Opens a connection to `rpc` and returns the channel for sending it requests.
    async fn open(&self, rpc: &Rpc) -> mpsc::UnboundedSender<Value> {
        let (ws_conn_incoming_tx, ws_conn_incoming_rx) = mpsc::unbounded_channel();
        ws_conn(
            rpc.clone(),
            self.rpc_list.clone(),
            ws_conn_incoming_rx,
            self.broadcast_tx.clone(),
            self.ws_error_tx.clone(),
        )
        .await;
        ws_conn_incoming_tx
    }

    /// Sends an incoming request to a WS connection.
    ///
    /// The RPC can be specified by its [`Rpc::id`] via `specified_node`,
    /// otherwise one is picked from `rpc_list`.
    async fn send(
        &mut self,
        incoming: Value,
        specified_node: Option<usize>,
        ws_buffer: &mut Vec<Value>,
    ) {
        let node_id = if let Some(node_id) = specified_node {
            node_id
        } else {
            let picked = {
                let mut rpc_list_guard = self.rpc_list.write().unwrap_or_else(|e| {
                    // Handle the case where the rpc_list RwLock is poisoned
                    tracing::error!(?e);
                    e.into_inner()
                });
                let (rpc, position) = pick(&mut rpc_list_guard);
                position.map(|_| rpc)
            };

            match picked {
                Some(rpc) => {
                    // An RPC that rejoined the list after the last reconnect has no connection yet.
                    if !self.handles.contains_key(&rpc.id()) {
                        let handle = self.open(&rpc).await;
                        self.handles.insert(rpc.id(), handle);
                    }
                    rpc.id()
                }
                None => {
                    // Check if the incoming content is a subscription.
                    //
                    // We do this because we want to send it to a buffer
                    // in case we have no available RPCs.
                    let method = &incoming["method"];
                    if method.eq(&EthRpcMethod::Subscription) || method.eq(&EthRpcMethod::Subscribe)
                    {
                        ws_buffer.push(incoming);
                    }
                    tracing::error!("No RPC position available");
                    return;
                }
            }
        };

        if let Some(ws) = self.handles.get(&node_id) {
            if ws.send(incoming).is_err() {
                tracing::error!("ws_conn_manager error: failed to send message");
            }
        } else {
            tracing::error!(node_id, "No WS connection for node");
        }
    }
}

/// Dispatches buffered WS subscriptions out to nodes.
///
/// Subscriptions that still can't be sent are buffered again.
async fn unload_buffer(connections: &mut WsConnections, ws_buffer: &mut Vec<Value>) {
    for incoming in std::mem::take(ws_buffer) {
        connections.send(incoming, None, ws_buffer).await;
    }
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
) {
    let index = rpc.id();
    let Some(ws_url) = &rpc.ws_url else {
        tracing::error!("Node {} has no WS endpoint!", rpc.name);
        return;
    };
    let ws_stream = match connect_async(ws_url).await {
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

    if !call.is_object() {
        return Ok(invalid_request_body!().to_string());
    }

    let id = call["id"].take();

    // Rewrite block tags before hashing: `latest` must be cached under the block it
    // resolved to, not under `latest`. Subscriptions have no block param to rewrite.
    call = replace_block_tags(&mut call, &cache_args.named_numbers);
    let call_string = call.to_string();
    let tx_hash = hash(call_string.as_bytes());

    if !has_block_tag(&call_string) {
        if let Ok(Some(mut rax)) = db_get!(cache_args.cache, tx_hash.as_bytes().to_owned().into()) {
            let mut cached: Value = from_slice(rax.as_mut()).unwrap();
            cached["id"] = id;
            return Ok(cached.to_string());
        }
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
        cache_query(&response.content.to_string(), call, tx_hash, cache_args).await;
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

    /// Connections whose handles go to the returned receivers, by [`Rpc::id`].
    fn mock_connections(
        rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    ) -> (
        WsConnections,
        HashMap<usize, mpsc::UnboundedReceiver<Value>>,
    ) {
        let (broadcast_tx, _) = broadcast::channel(10);
        let (ws_error_tx, _) = mpsc::unbounded_channel();
        let mut handles = HashMap::new();
        let mut receivers = HashMap::new();
        for rpc in rpc_list.read().unwrap().iter() {
            let (tx, rx) = mpsc::unbounded_channel();
            handles.insert(rpc.id(), tx);
            receivers.insert(rpc.id(), rx);
        }
        let connections = WsConnections {
            rpc_list: rpc_list.clone(),
            handles,
            broadcast_tx,
            ws_error_tx,
        };
        (connections, receivers)
    }

    #[tokio::test]
    async fn test_handle_incoming_message() {
        let rpc_list = create_mock_rpc_list().await;
        let id = rpc_list.read().unwrap()[1].id();
        let (mut connections, mut receivers) = mock_connections(&rpc_list);
        let incoming = json!({"type": "test"});
        let mut ws_buffer: Vec<Value> = Vec::new();

        connections
            .send(incoming.clone(), Some(id), &mut ws_buffer)
            .await;

        // Check if the message was sent through the channel
        let received = receivers.get_mut(&id).unwrap().recv().await;
        assert_eq!(received, Some(incoming));
    }

    #[tokio::test]
    async fn test_message_goes_to_picked_rpc_after_list_shifts() {
        let rpc_list = create_mock_rpc_list().await;
        let (mut connections, mut receivers) = mock_connections(&rpc_list);

        // The health check evicts the first RPC without reconnecting.
        let evicted = rpc_list.write().unwrap().remove(0);
        let remaining = rpc_list.read().unwrap()[0].id();

        let incoming = json!({"method": "eth_chainId"});
        connections
            .send(incoming.clone(), None, &mut Vec::new())
            .await;

        assert_eq!(
            receivers.get_mut(&remaining).unwrap().try_recv().ok(),
            Some(incoming)
        );
        assert!(receivers
            .get_mut(&evicted.id())
            .unwrap()
            .try_recv()
            .is_err());
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
    #[serial_test::serial]
    async fn test_ws_latest_is_cached_per_block() {
        let (incoming_tx, _incoming_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(10);
        let sub_data = Arc::new(SubscriptionData::new());
        let cache_args = CacheArgs::default();
        cache_args.named_numbers.write().unwrap().latest = 0x10;

        let call = || {
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": EthRpcMethod::Call,
                "params": [{"to": "0x00000000000000000000000000000000000000aa", "data": "0x"}, "latest"]
            })
        };
        let respond = |result: &'static str| {
            let broadcast_tx = broadcast_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let response = IncomingResponse {
                    content: json!({"jsonrpc": "2.0", "id": 1, "result": result}),
                    node_id: 0,
                };
                broadcast_tx.send(response).unwrap();
            });
        };

        respond("0xaaaa");
        let first = execute_ws_call(
            call(),
            1,
            &incoming_tx,
            broadcast_rx.resubscribe(),
            &sub_data,
            &cache_args,
        )
        .await
        .unwrap();
        assert!(first.contains("0xaaaa"));

        // The head moved, so `latest` must not be answered with block 0x10's result.
        cache_args.named_numbers.write().unwrap().latest = 0x11;
        respond("0xbbbb");
        let second = execute_ws_call(
            call(),
            1,
            &incoming_tx,
            broadcast_rx.resubscribe(),
            &sub_data,
            &cache_args,
        )
        .await
        .unwrap();
        assert!(second.contains("0xbbbb"), "got stale response {second}");
    }

    #[tokio::test]
    async fn test_ws_call_that_is_not_an_object() {
        let (incoming_tx, mut incoming_rx) = mpsc::unbounded_channel();
        let (_broadcast_tx, broadcast_rx) = broadcast::channel(10);
        let sub_data = Arc::new(SubscriptionData::new());
        let cache_args = CacheArgs::default();

        for call in [
            json!([{"jsonrpc": "2.0", "id": 1, "method": "eth_chainId"}]),
            json!(1),
            json!("eth_chainId"),
        ] {
            let response = execute_ws_call(
                call,
                1,
                &incoming_tx,
                broadcast_rx.resubscribe(),
                &sub_data,
                &cache_args,
            )
            .await
            .unwrap();
            let response: Value = serde_json::from_str(&response).unwrap();
            assert_eq!(response["error"]["code"], -32600);
        }
        assert!(
            incoming_rx.try_recv().is_err(),
            "nothing is forwarded upstream"
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
