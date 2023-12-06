use crate::{
    Rpc,
    balancer::selection::select::pick,
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message
};

use serde_json::Value;

use rand::random;

use std::{
    format,
    sync::{
        Arc,
        RwLock,
    },
};

use tokio::{
    sync::{
        mpsc,
        watch,
    },
};
use futures_util::{
    SinkExt,
};

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// Open WS connections to our nodes and accept and process internal WS calls
// whenever we receive something from incoming_rx
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    outgoing_tx: watch::Sender<Value>,
) {
    println!("ws_conn_manager");

    let rpc_list_clone = rpc_list.read().unwrap().clone();

    let mut ws_handles = Vec::new();
    for rpc in rpc_list_clone {
        let url = reqwest::Url::parse(&rpc.ws_url.unwrap()).unwrap();

        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to WS");
        // push clients to ws_handles
        ws_handles.push(ws_stream);
    }

    // continuously listen for incoming messages
    loop {
        let incoming = incoming_rx.recv().await.unwrap();
        println!("ws received incoming message: {}", incoming);

        // Get the index of the fastest node from rpc_list
        let rpc_position;
        {
            let mut rpc_list_guard = rpc_list.write().unwrap();
            (_, rpc_position) = pick(&mut rpc_list_guard);
        }

        // Send request to ws_handles[rpc_position]
        let a = ws_handles[rpc_position.unwrap_or(0)]
            .send(Message::Text(incoming.to_string()))
            .await;

        // send the response to outgoing_tx
        match a {
            Ok(_) => {
                println!("ws_conn_manager: sent message to ws");
                outgoing_tx.send(incoming).unwrap();
            }
            Err(e) => {
                println!("ws_conn_manager error: {}", e);
            }
        }
    }
}

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(
    call: String,
    incoming_tx: mpsc::UnboundedSender<Value>,
    mut outgoing_rx: watch::Receiver<Value>,
) -> Result<String, Error> {
    // Convert `call` to value
    let mut call_val: Value = serde_json::from_str(&call).unwrap();

    // Store id of call and set random id we'll actually forward to the node
    //
    // We'll use the random id to look at which call is ours when watching for updates
    let id = call_val["id"].clone();
    let rand_id = random::<u32>();
    call_val["id"] = rand_id.into();

    println!("ws SEND");

    // Send call to ws_conn_manager
    match incoming_tx.send(call_val) {
        Ok(_) => {}
        Err(e) => {
            println!("ws_conn_manager error: {}", e);
        }
    };

    println!("ws SENT");

    // Wait for response from ws_conn_manager
    let mut response = outgoing_rx.wait_for(|v| v["id"] == rand_id).await.unwrap().to_owned();

    response["id"] = id;

    Ok(format!("Hello from blutgang!: {:?}", response))
}
