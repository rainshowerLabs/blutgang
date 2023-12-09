use crate::balancer::processing::CacheArgs;
use serde_json::Value;

use tokio::sync::{
    broadcast,
    mpsc,
};

use crate::websocket::client::execute_ws_call;

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
    cache_args: &CacheArgs,
) -> Result<(), Error> {
    let mut websocket = websocket.await?;

    while let Some(message) = websocket.next().await {
        match message? {
            Message::Text(msg) => {
                println!("\x1b[35mInfo:\x1b[0m Received WS text message: {msg}");

                // Forward the message to the best available RPC
                let resp = execute_ws_call(
                    msg,
                    incoming_tx.clone(),
                    outgoing_rx.resubscribe(),
                    cache_args,
                )
                .await?;

                websocket.send(Message::text(resp)).await?;
            }
            Message::Close(msg) => {
                if let Some(msg) = &msg {
                    println!(
                        "\x1b[35mInfo:\x1b[0mReceived close message with code {} and message: {}",
                        msg.code, msg.reason
                    );
                } else {
                    println!("Received close message");
                }
            }
            Message::Ping(_) | Message::Pong(_) => {}
            _ => {
                websocket
                    .send(Message::text(
                        "Wrn: Unsupported message format, please use text!",
                    ))
                    .await?;
            }
        }
    }

    Ok(())
}
