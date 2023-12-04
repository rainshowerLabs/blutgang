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
        RwLock,
    },
};

use tokio::sync::mpsc;

pub async fn connection_manager(
    wsreg_rx: mpsc::Receiver<Wsreg>,
    ws_connections: Arc<RwLock<HashMap<usize, watch::Receiver<Value>>>>,
) -> Result<(), HealthError> {
    unimplemented!()
}
