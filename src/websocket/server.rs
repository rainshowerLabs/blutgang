use dashmap::DashMap;
use std::sync::Arc;

use crate::{
    balancer::processing::CacheArgs,
    websocket::{
        client::execute_ws_call,
        subscription_manager::RequestResult,
    },
};

use rand::random;
use serde_json::Value;

use tokio::sync::{
    broadcast,
    mpsc,
};

use futures::{
    sink::SinkExt,
    stream::StreamExt,
};

use hyper_tungstenite::{
    tungstenite,
    HyperWebsocket,
};
use tungstenite::Message;

// Recommended way to deal with this, idk either
type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Handle a websocket connection.
pub async fn serve_websocket(
    websocket: HyperWebsocket,
    incoming_tx: mpsc::UnboundedSender<Value>,
    outgoing_rx: broadcast::Receiver<Value>,
    sink_map: Arc<DashMap<u64, mpsc::UnboundedSender<RequestResult>>>,

    cache_args: CacheArgs,
) -> Result<(), Error> {
    let websocket = websocket.await?;

    // Split the Sink so we can do async send/recv
    let (mut websocket_sink, mut websocket_stream) = websocket.split();

    // Create channels for message send/receiving
    let (tx, mut rx) = mpsc::unbounded_channel::<RequestResult>();

    // Generate an id for our user
    //
    // We use this to identify which requests are for us
    let user_id = random::<u64>();

    // Add the user to the sink map
    sink_map.insert(user_id, tx.clone());

    // Spawn taks for sending messages to the client
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Forward the message to the best available RPC
            let resp = execute_ws_call(
                msg.into(),
                user_id,
                incoming_tx.clone(),
                outgoing_rx.resubscribe(),
                &cache_args,
            )
            .await
            .unwrap_or("{\"error\": \"Failed to execute call\"}".to_string());

            websocket_sink
                .send(Message::text::<String>(resp))
                .await
                .unwrap()
        }
    });

    while let Some(message) = websocket_stream.next().await {
        match message? {
            Message::Text(mut msg) => {
                println!("\x1b[35mInfo:\x1b[0m Received WS text message: {msg}");
                // Send message to the channel
                tx.send(RequestResult::Call(unsafe {
                    simd_json::from_str(&mut msg)?
                }))
                .unwrap();
            }
            Message::Close(msg) => {
                // Remove the user from the sink map
                sink_map.remove(&user_id);
                if let Some(msg) = &msg {
                    println!(
                        "\x1b[35mInfo:\x1b[0mReceived close message with code {} and message: {}",
                        msg.code, msg.reason
                    );
                } else {
                    println!("Received close message");
                }
            }
            _ => {}
        }
    }

    Ok(())
}
