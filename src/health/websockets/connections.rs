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

// Receives internal WS messages, processes them and sends them to the fastest available node
pub async fn forward_connection(
    wsreg_rx: mpsc::Receiver<Wsreg>,
    ws_incoming: broadcast::Receiver<Value>,
    ws_outgoing: Arc<HashMap<usize, watch::Sender<Value>>>,
) -> Result<(), HealthError> {


    Ok(())
}
