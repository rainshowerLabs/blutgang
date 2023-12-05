use crate::Rpc;
use ezsockets::ClientConfig;

use serde_json::Value;

use rand::random;

use async_trait::async_trait;
use std::{
    format,
    sync::{
        Arc,
        RwLock,
    },
};

use tokio::sync::{
    mpsc,
    watch,
};

pub struct Client {
    pub handle: ezsockets::Client<Self>,
}

#[async_trait]
impl ezsockets::ClientExt for Client {
    type Call = ();

    async fn on_text(&mut self, text: String) -> Result<(), ezsockets::Error> {
        println!("received message: {}", text);
        Ok(())
    }

    async fn on_binary(&mut self, bytes: Vec<u8>) -> Result<(), ezsockets::Error> {
        println!("received bytes: {:?}", bytes);
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), ezsockets::Error> {
        let () = call;
        println!("received call: {:?}", call);
        Ok(())
    }
}

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

        let config = ClientConfig::new(url);
        let (handle, future) = ezsockets::connect(|handle| Client { handle }, config).await;
        tokio::spawn(async move {
            future.await.unwrap();
        });
        ws_handles.push(handle);
    }

    // continously listen for incoming messages
    loop {
        let incoming = incoming_rx.recv().await.unwrap();

        println!("ws received incoming message: {}", incoming);
        let _ = outgoing_tx.send(incoming);
    }
}

// Receive JSON-RPC call from balancer thread and respond with ws response
pub async fn execute_ws_call(
    call: String,
    incoming_tx: mpsc::UnboundedSender<Value>,
    mut outgoing_rx: watch::Receiver<Value>,
) -> Result<String, ezsockets::Error> {
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
    
    println!("waiting for response");

    // wait for response["id"] == rand_id 
    let mut response = outgoing_rx.wait_for(|v| v["id"] == rand_id).await.unwrap().to_owned();

    response["id"] = id;

    Ok(format!("Hello from blutgang!: {:?}", response))
}
