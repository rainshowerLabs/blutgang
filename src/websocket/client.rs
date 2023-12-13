use crate::{
    balancer::{
        format::replace_block_tags,
        processing::{
            cache_querry,
            CacheArgs,
        },
        selection::select::pick,
    },
    rpc::types::hex_to_decimal,
    websocket::{
        subscription_manager::insert_and_return_subscription,
        types::{
            WsChannelErr,
            WsconnMessage,
        },
    },
    Rpc,
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message,
};

use serde_json::Value;

use std::{
    println,
    sync::{
        Arc,
        RwLock,
    },
};

// Select either blake3 or xxhash based on the features
#[cfg(not(feature = "xxhash"))]
use blake3::hash;

#[cfg(feature = "xxhash")]
use xxhash_rust::xxh3::xxh3_64;
#[cfg(feature = "xxhash")]
use zerocopy::AsBytes; // Impls AsBytes trait for u64

use futures_util::{
    SinkExt,
    StreamExt,
};
use tokio::sync::{
    broadcast,
    mpsc,
};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// Open WS connections to our nodes and accept and process internal WS calls
// whenever we receive something from incoming_rx
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<WsconnMessage>,
    broadcast_tx: broadcast::Sender<Value>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
) {
    // We want to create a ws connection for each rpc in rpc_list
    // We also want to have a corresponding channel and put it in a Vec
    let mut ws_handles = create_ws_vec(&rpc_list, &broadcast_tx, ws_error_tx.clone()).await;

    // continuously listen for incoming messages
    loop {
        let incoming: Value = match incoming_rx.recv().await.unwrap() {
            WsconnMessage::Message(incoming) => incoming,
            WsconnMessage::Reconnect() => {
                // Clear everything in ws_handles and reconnect
                ws_handles = create_ws_vec(&rpc_list, &broadcast_tx, ws_error_tx.clone()).await;
                continue;
            }
        };

        // Get the index of the fastest node from rpc_list
        let rpc_position;
        {
            let mut rpc_list_guard = rpc_list.write().unwrap();
            (_, rpc_position) = pick(&mut rpc_list_guard);
        }

        // Error if rpc_position is None
        let rpc_position = if let Some(rpc_position) = rpc_position {
            rpc_position
        } else {
            println!("ws_conn_manager error: no rpc_position");
            continue;
        };

        // Send message to the corresponding ws_conn
        match &ws_handles[rpc_position] {
            Some(ws) => {
                match ws.send(incoming) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("ws_conn_manager error: {}", e);
                    }
                };
            }
            None => {
                println!("No WS connection at that index!");
            }
        };
    }
}

// Given a list of rpcs, create a vec of their WS connections
pub async fn create_ws_vec(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    broadcast_tx: &broadcast::Sender<Value>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
) -> Vec<Option<mpsc::UnboundedSender<Value>>> {
    let rpc_list_clone = rpc_list.read().unwrap().clone();

    // We want to create a ws connection for each rpc in rpc_list
    // We also want to have a corresponding channel and put it in a Vec
    let mut ws_handles = Vec::new();
    for (i, rpc) in rpc_list_clone.iter().enumerate() {
        let (ws_conn_incoming_tx, ws_conn_incoming_rx) = mpsc::unbounded_channel();

        ws_handles.push(Some(ws_conn_incoming_tx));

        ws_conn(
            rpc.clone(),
            ws_conn_incoming_rx,
            broadcast_tx.clone(),
            ws_error_tx.clone(),
            i,
        )
        .await;
    }

    ws_handles
}

// Creates a task makes a new ws connection, listens to incoming messages and
// returns them via a channel
pub async fn ws_conn(
    rpc: Rpc,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    outgoing_rx: broadcast::Sender<Value>,
    ws_error_tx: mpsc::UnboundedSender<WsChannelErr>,
    index: usize,
) {
    let url = reqwest::Url::parse(&rpc.ws_url.unwrap()).unwrap();
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to WS");

    let (mut write, mut read) = ws_stream.split();

    // Create a task for sending messages to RPC
    tokio::spawn(async move {
        // continuously listen for incoming messages
        loop {
            let incoming = incoming_rx.recv().await.unwrap();

            // add close connection functionality
            // TODO: this type should be an enum
            if incoming["method"] == "close" {
                let _ = write.close().await;
                break;
            }

            // Send request to ws_stream
            let _ = write.send(Message::Text(incoming.to_string())).await;
        }
    });

    // Create task for continously reading responses we got from our node and broadcasting them
    tokio::spawn(async move {
        loop {
            // TODO: if read is dropped that means that this connection shat itself.
            // We should then be alerting our health checker that it died.
            let rax = read.next().await;
            match rax {
                Some(Ok(rax)) => {
                    let rax =
                        unsafe { simd_json::from_str(&mut rax.into_text().unwrap()).unwrap() };
                    outgoing_rx.send(rax).unwrap();
                }
                Some(Err(e)) => {
                    println!("ws_conn error: couldnt get response!: {}", e);
                }
                None => {
                    let _ = ws_error_tx.send(WsChannelErr::Closed(index));
                }
            }
        }
    });
}

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(
    mut call: Value,
    user_id: u64,
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
    broadcast_rx: broadcast::Receiver<Value>,
    cache_args: &CacheArgs,
) -> Result<String, Error> {
    // Store id of call and set random id we'll actually forward to the node
    //
    // We'll use the random id to look at which call is ours when watching for updates
    let id = call["id"].take();

    // Hash the request with either blake3 or xxhash depending on the enabled feature
    let tx_hash;
    #[cfg(not(feature = "xxhash"))]
    {
        tx_hash = hash(call.to_string().as_bytes());
    }
    #[cfg(feature = "xxhash")]
    {
        tx_hash = xxh3_64(call.to_string().as_bytes());
    }

    // Check if we have a cached response
    // TODO: responses arent shared??
    match cache_args.cache.get(tx_hash.as_bytes()) {
        Ok(Some(mut rax)) => {
            let mut cached: Value = simd_json::serde::from_slice(&mut rax).unwrap();
            cached["id"] = id;

            return Ok(cached.to_string());
        }
        Ok(None) => {}

        Err(e) => {
            println!("Error getting cached response: {}", e);
        }
    }

    // Check if our call was a subscription
    let is_subscription = call["method"] == "eth_subscribe";

    if is_subscription {
        // Check if our tx_hash exists in the sled "subscriptions" subtree
        let subscriptions = cache_args.cache.open_tree("subscriptions")?;

        match subscriptions.get(tx_hash.as_bytes()) {
            Ok(Some(mut rax)) => {
                let mut cached: Value = simd_json::serde::from_slice(&mut rax).unwrap();
                cached["id"] = id;
                let subscription_id = hex_to_decimal(cached["result"].as_str().unwrap())?;

                if let Some(subscribed_users) = &cache_args.subscribed_users {
                    subscribed_users
                        .entry(subscription_id)
                        .or_default()
                        .insert(user_id, true);
                }

                return Ok(cached.to_string());
            }
            Ok(None) => {}
            Err(e) => println!("Error accesssing subtree: {}", e),
        }
    } else {
        // Rewrite any parameters we can
        call = replace_block_tags(&mut call, &cache_args.named_numbers);
    }

    // Replace call id with our user id
    call["id"] = user_id.into();

    // Send call to ws_conn_manager
    match incoming_tx.send(WsconnMessage::Message(call.clone())) {
        Ok(_) => {}
        Err(e) => {
            println!("ws_conn_manager error: {}", e);
        }
    };

    let mut response = listen_for_response(user_id, broadcast_rx).await?;

    // Cache if possible
    if is_subscription {
        let _ = insert_and_return_subscription(tx_hash, response.clone(), cache_args);
        let subscription_id = hex_to_decimal(response["result"].as_str().unwrap())?;

        // Register the subscription id with the user
        //
        // We'll have to append or create the vec
        if let Some(subscribed_users) = &cache_args.subscribed_users {
            subscribed_users
                .entry(subscription_id)
                .or_default()
                .insert(user_id, true);
        }
    } else {
        cache_querry(&mut response.to_string(), call, tx_hash, cache_args);
    }

    // Set id to the original id
    response["id"] = id;

    Ok(response.to_string())
}

// Listens for responses that match our id on the broadcast channel
async fn listen_for_response(
    user_id: u64,
    mut broadcast_rx: broadcast::Receiver<Value>,
) -> Result<Value, Error> {
    // Wait until we get a response matching our id
    let mut response = broadcast_rx
        .recv()
        .await
        .expect("Failed to receive response from WS");

    while response["id"] != <u64 as Into<Value>>::into(user_id) {
        response = broadcast_rx
            .recv()
            .await
            .expect("Failed to receive response from WS");
    }

    Ok(response)
}
