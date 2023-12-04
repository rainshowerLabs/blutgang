use crate::health::websockets::types::WsConnection;
use crate::Rpc;
use std::sync::{RwLock, atomic::AtomicUsize};
use crate::health::{
    error::HealthError,
    websockets::types::Wsreg,
};
use serde_json::Value;
use tokio::sync::watch;

use std::{
    collections::HashMap,
    sync::{
        Arc,
    },
};

use tokio::sync::{
    mpsc,
    broadcast,
};

// Creates the WS connections and puts them in a RwLock<Vec>
pub async fn update_ws(
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    wscon_list: Arc<HashMap<AtomicUsize, RwLock<WsConnection>>>
) -> Result<(), HealthError> {


    Ok(())
}

// Receives internal WS messages, processes them and sends them to the fastest available node
pub async fn forward_connection(
    wsreg_rx: mpsc::Receiver<Wsreg>,
    ws_incoming: broadcast::Receiver<Value>,
    ws_outgoing: Arc<HashMap<usize, watch::Sender<Value>>>,
) -> Result<(), HealthError> {


    Ok(())
}
