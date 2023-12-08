use crate::{
    balancer::selection::select::pick,
    Rpc,
};

use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::Message,
};

use serde_json::Value;

use rand::random;

use std::{
    println,
    sync::{
        Arc,
        RwLock,
    },
};

use sled::Db;

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

struct ConnectionChannels {
    incoming_tx: mpsc::UnboundedSender<Value>,
}

// Open WS connections to our nodes and accept and process internal WS calls
// whenever we receive something from incoming_rx
pub async fn ws_conn_manager(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    mut incoming_rx: mpsc::UnboundedReceiver<Value>,
    broadcast_tx: broadcast::Sender<Value>,
) {
    println!("ws_conn_manager");

    let rpc_list_clone = rpc_list.read().unwrap().clone();

    // We want to create a ws connection for each rpc in rpc_list
    // We also want to have a corresponding channel and put it in a Vec
    let mut ws_handles = Vec::new();
    for rpc in rpc_list_clone {
        let (ws_conn_incoming_tx, ws_conn_incoming_rx) = mpsc::unbounded_channel();

        let connections = ConnectionChannels {
            incoming_tx: ws_conn_incoming_tx,
        };

        ws_handles.push(connections);

        ws_conn(rpc, ws_conn_incoming_rx, broadcast_tx.clone()).await;
    }

    // continuously listen for incoming messages
    loop {
        let incoming = incoming_rx.recv().await.unwrap();

        // Get the index of the fastest node from rpc_list
        let rpc_position;
        {
            let mut rpc_list_guard = rpc_list.write().unwrap();
            (_, rpc_position) = pick(&mut rpc_list_guard);
        }

        println!("ws_handles len: {:?}", ws_handles.len());

        // Error if rpc_position is None
        let rpc_position = if let Some(rpc_position) = rpc_position {
            rpc_position
        } else {
            println!("ws_conn_manager error: no rpc_position");
            continue;
        };

        // Send message to the corresponding ws_conn
        match ws_handles[rpc_position].incoming_tx.send(incoming) {
            Ok(_) => {}
            Err(e) => {
                println!("ws_conn_manager error: {}", e);
            }
        };
    }
}

// Creates a task makes a new ws connection, listens to incoming messages and
// returns them via a channel
pub async fn ws_conn(
    rpc: Rpc,
    mut incoming_tx: mpsc::UnboundedReceiver<Value>,
    outgoing_rx: broadcast::Sender<Value>,
) {
    let url = reqwest::Url::parse(&rpc.ws_url.unwrap()).unwrap();

    tokio::spawn(async move {
        let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to WS");

        let (mut write, mut read) = ws_stream.split();

        // continuously listen for incoming messages
        loop {
            println!("HELLO!");
            let incoming = incoming_tx.recv().await.unwrap();
            println!("HELLO!!!!!!!!");

            // add close connection functionality
            // TODO: this type should be an enum
            if incoming["method"] == "close" {
                let _ = write.close().await;
                break;
            }

            // Send request to ws_stream
            let _ = write.send(Message::Text(incoming.to_string())).await;

            // get the response from ws_stream
            let rax = read.next().await.unwrap();

            // send the response to outgoing_tx
            match rax {
                Ok(rax) => {
                    println!("ws_conn: sent message to ws: {}", rax);
                    let rax = serde_json::from_str(&rax.into_text().unwrap()).unwrap();
                    outgoing_rx.send(rax).unwrap();
                }
                Err(e) => {
                    println!("ws_conn error: couldnt get response!: {}", e);
                }
            }
        }
    });
}

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(
    call: String,
    incoming_tx: mpsc::UnboundedSender<Value>,
    mut broadcast_rx: broadcast::Receiver<Value>,
    cache: Arc<Db>,
) -> Result<String, Error> {
    // Convert `call` to value
    let mut call_val: Value = serde_json::from_str(&call).unwrap();

    // Store id of call and set random id we'll actually forward to the node
    //
    // We'll use the random id to look at which call is ours when watching for updates
    let id = call_val["id"].clone();

    // Hash the request with either blake3 or xxhash depending on the enabled feature
    let tx_hash;
    #[cfg(not(feature = "xxhash"))]
    {
        tx_hash = hash(call_val.to_string().as_bytes());
    }
    #[cfg(feature = "xxhash")]
    {
        tx_hash = xxh3_64(call_val.to_string().as_bytes());
    }

    // Check if we have a cached response
    match cache.get(tx_hash.as_bytes()) {
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

    let rand_id = random::<u32>();
    call_val["id"] = rand_id.into();

    // Send call to ws_conn_manager
    match incoming_tx.send(call_val) {
        Ok(_) => {}
        Err(e) => {
            println!("ws_conn_manager error: {}", e);
        }
    };

    // Wait until we get a response matching our id
    let mut response = broadcast_rx
        .recv()
        .await
        .expect("Failed to receive response from WS");

    if response["id"] != rand_id {
        while response["id"] != id {
            response = broadcast_rx
                .recv()
                .await
                .expect("Failed to receive response from WS");
        }
    }

    // Set id to the original id
    response["id"] = id;

    Ok(response.to_string())
}
