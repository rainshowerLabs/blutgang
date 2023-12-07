use std::collections::HashMap;
use crate::{
    balancer::selection::select::pick,
    Rpc,
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message, 
    WebSocketStream, 
    MaybeTlsStream,
};

use serde_json::Value;

use std::{
    format,
    sync::{
        Arc,
        RwLock,
    },
};

use futures_util::{
    SinkExt,
    StreamExt,
};
use tokio::{
    net::TcpStream,
    sync::{
        mpsc,
        watch,
    },
};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// Open WS connections to our nodes and accept and process internal WS calls
// whenever we receive something from incoming_rx
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    ws_registry: Arc<RwLock<HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    outgoing_tx: watch::Sender<Value>,
) {
    println!("ws_conn_manager");

    // continuously listen for incoming channels getting opened
    loop {
        let incoming = incoming_rx.recv().await.unwrap();
        let channel_id = incoming["id"].as_u64().unwrap();

        // Check if we have a ws connection open for our channel, if we dont open one
        let mut ws_registry_guard = ws_registry.write().unwrap();
        if !ws_registry_guard.contains_key(&channel_id) {
            println!("ws_conn_manager: registering new ws conn");
            register_ws_conn(rpc_list.clone(), channel_id, ws_registry.clone()).await.unwrap();
        }

        // Send the message to the ws connection
        let ws_con = ws_registry_guard.get_mut(&channel_id).unwrap();
        let _ = ws_con.send(Message::Text(incoming.to_string()));

        let rax = ws_con.next().await.unwrap();

        match rax {
            Ok(rax) => {
                println!("ws_conn_manager: sent message to ws");
                let rax = serde_json::from_str(&rax.into_text().unwrap()).unwrap();
                outgoing_tx.send(rax).unwrap();
            }
            Err(e) => {
                println!("ws_conn_manager error: couldnt get response!: {}", e);
            }
        }
    }
}

// Register a connection for a specific channel ID to an RPC ws endpoint
pub async fn register_ws_conn(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    channel_id: u64,
    ws_registry: Arc<RwLock<HashMap<u64, WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
) -> Result<(), Error> {

    // Get the index of the fastest node from rpc_list
    let rpc_position;
    {
        let mut rpc_list_guard = rpc_list.write().unwrap();
        (_, rpc_position) = pick(&mut rpc_list_guard);
    }

    let url = rpc_list.read().unwrap()[rpc_position.unwrap()].ws_url.clone();

    // Open a WS connection to the RPC
    let (ws_stream, _) = connect_async(url.unwrap()).await.expect("Failed to connect to WS");

    // Register the channel
    {
        let mut ws_reg_guard = ws_registry.write().unwrap();
        ws_reg_guard.insert(channel_id, ws_stream);
    }

    Ok(())
}

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(
    call: String,
    channel_id: u64,
    incoming_tx: mpsc::UnboundedSender<Value>,
    mut outgoing_rx: watch::Receiver<Value>,
) -> Result<String, Error> {
    // Convert `call` to value
    let mut call_val: Value = serde_json::from_str(&call).unwrap();

    // Store id of call and set random id we'll actually forward to the node
    //
    // We'll use the random id to look at which call is ours when watching for updates
    let id = call_val["id"].clone();
    call_val["id"] = channel_id.into();

    // Send call to ws_conn_manager
    match incoming_tx.send(call_val) {
        Ok(_) => {}
        Err(e) => {
            println!("ws_conn_manager error: {}", e);
        }
    };

    // Wait for response from ws_conn_manager
    let mut response = outgoing_rx
        .wait_for(|v| v["id"] == channel_id)
        .await
        .unwrap()
        .to_owned();

    response["id"] = id;

    Ok(format!("Hello from blutgang!: {:?}", response))
}
