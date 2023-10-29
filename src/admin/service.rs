// use sled::Db;
use crate::{
    admin::admin_functions::execute_method,
    Rpc,
};

use serde_json::Value;
use simd_json;

use std::sync::{
    Arc,
    RwLock,
};

pub fn process_request(
    rx: &str,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    // cache: Arc<Db>,
) -> String {
    // TODO: this is a tiny bit retarded
    let mut admin_msg_str = rx.to_string();
    let val: Value = unsafe { simd_json::serde::from_str(&mut admin_msg_str).unwrap_or_default() };

    let resp = match execute_method(
        val["method"].as_str(),
        val["params"].as_str(),
        rpc_list,
        poverty_list,
        // cache,
    ) {
        Ok(resp) => resp,
        Err(resp) => format!("{{\"error\":\"{}\"}}", resp),
    };

    resp
}
