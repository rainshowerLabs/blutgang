use crate::{
    admin::error::AdminError,
    Rpc,
    Settings,
};

use std::{
    ptr,
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
};

use serde_json::{
    json,
    Value,
    Value::Null,
};

use sled::Db;

pub async fn execute_method(
    tx: Value,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    config: Arc<RwLock<Settings>>,
    cache: Arc<Db>,
) -> Result<Value, AdminError> {
    let method = tx["method"].as_str();
    match method {
        Some("blutgang_quit") => admin_blutgang_quit(cache).await,
        Some("blutgang_rpc_list") => admin_list_rpc(rpc_list),
        Some("blutgang_flush_cache") => admin_flush_cache(cache).await,
        Some("blutgang_config") => admin_config(config),
        Some("blutgang_poverty_list") => admin_list_rpc(poverty_list),
        Some("blutgang_add_to_rpc_list") => {
            admin_add_rpc(rpc_list, tx["params"].as_array())
        },
        Some("blutgang_add_to_poverty_list") => {
            admin_add_rpc(poverty_list, tx["params"].as_array())
        },
        Some("blutgang_remove_from_rpc_list") => {
            admin_remove_rpc(rpc_list, tx["params"].as_array())
        },
        Some("blutgang_remove_from_poverty_list") => {
            admin_remove_rpc(poverty_list, tx["params"].as_array())
        },
        // "blutgang_db_stats" => _,
        // "blutgang_print_db_profile_and_drop" => _,
        // "blutgang_cache" => _,
        // "blutgang_force_reorg" => _,
        // "blutgang_force_health" => _,
        // _ => println!("\x1b[31mErr:\x1b[0m Invalid admin namespace method"),
        _ => Err(AdminError::InvalidMethod),
    }
}

// Quit Blutgang upon receiving this method
// We're returning a Null and allowing unreachable code so rustc doesnt cry
#[allow(unreachable_code)]
async fn admin_blutgang_quit(cache: Arc<Db>) -> Result<Value, AdminError> {
    // We're doing something not-good so flush everything to disk
    let _ = cache.flush_async().await;
    // Drop cache so we get the print profile on drop thing before we quit
    // We have to get the raw pointer
    // TODO: This still doesnt work!
    unsafe {
        ptr::drop_in_place(Arc::into_raw(cache) as *mut Db);
    }
    std::process::exit(0);
    Ok(Value::Null)
}

async fn admin_flush_cache(cache: Arc<Db>) -> Result<Value, AdminError> {
    let time = Instant::now();
    let _ = cache.flush_async().await;
    let time = time.elapsed();

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": format!("Cache flushed in {:?}", time),
    });

    Ok(rx)
}

fn admin_config(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let mut clone = guard.clone();
    clone.admin.token = "HIDDEN".to_string(); // Hide the token
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": format!("{:?}", clone),
    });

    Ok(rx)
}

// List generic Fn to list RPCs from a Arc<RwLock<Vec<Rpc>>>
// Used for `blutgang_rpc_list` and `blutgang_poverty_list`
fn admin_list_rpc(rpc_list: &Arc<RwLock<Vec<Rpc>>>) -> Result<Value, AdminError> {
    let rpc_list = rpc_list.read().map_err(|_| AdminError::Innacessible)?;
    let mut rpc_list_str = String::new();

    rpc_list_str.push_str("[");
    for (i, rpc) in rpc_list.iter().enumerate() {
        println!("RPC {}:\n{:#?}", i, rpc);
        rpc_list_str.push_str(&format!("\"{:?}\", ", rpc));
    }
    rpc_list_str.push_str("]");

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": rpc_list_str,
    });

    Ok(rx)
}

// Pushes an RPC to the end of the list
//
// param[0] - RPC url
// param[1] - max_consecutive
// param[2] - ma_len
fn admin_add_rpc(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    params: Option<&Vec<Value>>,
) -> Result<Value, AdminError> {
    let params = match params {
        Some(params) => params,
        None => return Err(AdminError::InvalidParams),
    };

    if params.len() != 3 {
        return Err(AdminError::InvalidParams);
    }

    let rpc = match params[0].as_str() {
        Some(rpc) => rpc,
        None => return Err(AdminError::InvalidParams),
    };

    let mut rpc_list = rpc_list.write().map_err(|_| AdminError::Innacessible)?;
    rpc_list.push(Rpc::new(
        rpc.to_string(),
        params[1].as_u64().unwrap_or(0) as u32,
        params[2].as_f64().unwrap_or(0.0),
    ));

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": "Success",
    });

    Ok(rx)
}

// Remove RPC at a specified index
//
// param[0] - RPC index
fn admin_remove_rpc(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    params: Option<&Vec<Value>>,
) -> Result<Value, AdminError> {
    let params = match params {
        Some(params) => params,
        None => return Err(AdminError::InvalidParams),
    };

    if params.len() != 1 {
        return Err(AdminError::InvalidParams);
    }

    let index = match params[0].as_u64() {
        Some(index) => index,
        None => return Err(AdminError::InvalidParams),
    };

    let mut rpc_list = rpc_list.write().map_err(|_| AdminError::Innacessible)?;
    rpc_list.remove(index as usize);

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": "Success",
    });

    Ok(rx)
}
