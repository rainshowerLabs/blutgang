use dashmap::DashMap;
use std::sync::Arc;

use crate::{
    balancer::processing::CacheArgs,
    websocket::{
        client::execute_ws_call,
        types::{
            RequestResult,
            WsconnMessage,
        },
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
    incoming_tx: mpsc::UnboundedSender<WsconnMessage>,
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
    println!("\x1b[35mInfo:\x1b[0m Adding user {} to sink map", user_id);
    sink_map.insert(user_id, tx.clone());

    let sink_map_clone = sink_map.clone();

    // Spawn taks for sending messages to the client
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Forward the message to the best available RPC
            //
            // If we received a subscription, just send it to the client
            match msg {
                RequestResult::Call(call) => {
                    let resp = execute_ws_call(
                        call,
                        user_id,
                        incoming_tx.clone(),
                        outgoing_rx.resubscribe(),
                        &cache_args,
                    )
                    .await
                    .unwrap_or("{\"error\": \"Failed to execute call\"}".to_string());

                    match websocket_sink.send(Message::text::<String>(resp)).await {
                        Ok(_) => {},
                        Err(e) => {
                            // Remove the user from the sink map
                            sink_map_clone.remove(&user_id);
                            println!("\x1b[93mWrn:\x1b[0m Error sending call: {}", e);
                            break;
                        }
                    }
                }
                RequestResult::Subscription(sub) => {
                    match websocket_sink.send(Message::text::<String>(sub.to_string())).await {
                        Ok(_) => {},
                        Err(e) => {
                            // Remove the user from the sink map
                            sink_map_clone.remove(&user_id);
                            println!("\x1b[93mWrn:\x1b[0m Error sending call: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    while let Some(message) = websocket_stream.next().await {
        match message {
            Ok(Message::Text(mut msg)) => {
                println!("\x1b[35mInfo:\x1b[0m Received WS text message: {msg}");
                // Send message to the channel
                tx.send(RequestResult::Call(unsafe {
                    simd_json::from_str(&mut msg)?
                }))
                .unwrap();
            }
            Ok(Message::Close(msg)) => {
                if let Some(msg) = &msg {
                    println!(
                        "\x1b[35mInfo:\x1b[0mReceived close message with code {} and message: {}",
                        msg.code, msg.reason
                    );
                } else {
                    println!("Received close message");
                }
            }
            Err(e) => {
                // Remove the user from the sink map
                sink_map.remove(&user_id);
                println!("\x1b[93mWrn:\x1b[0m Error receiving message: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
