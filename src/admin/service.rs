use sled::Db;
use crate::{
    admin::admin_functions::execute_method,
    Rpc,
};

use serde_json::Value;

use std::sync::{
    Arc,
    RwLock,
};

pub async fn process_request(
    tx: Value,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    cache: Arc<Db>,
) -> String {
    let resp = match execute_method(
        tx["method"].as_str(),
        tx["params"].as_str(),
        rpc_list,
        cache,
    ).await {
        Ok(resp) => resp,
        Err(resp) => format!("{{\"error\":\"{}\"}}", resp),
    };

    resp
}
