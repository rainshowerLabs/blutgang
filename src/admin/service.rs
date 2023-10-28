use crate::Rpc;

use serde_json::Value;
use simd_json;

use std::sync::{
    Arc,
    RwLock,
};

use tokio::sync::mpsc::UnboundedReceiver;
use tokio_stream::{
    wrappers::UnboundedReceiverStream,
    StreamExt,
};

use super::admin_functions::execute_method;

pub async fn admin_service(
    // rpc_list: Arc<RwLock<Vec<Rpc>>>,
    // poverty_list: Arc<RwLock<Vec<Rpc>>>,
    rx: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: this is a tiny bit retarded
    let mut admin_msg_str = rx.to_string();
    let val: Value = unsafe { simd_json::serde::from_str(&mut admin_msg_str)? };
    execute_method(val["method"].as_str(), val["params"].as_str());

    Ok(())
}
