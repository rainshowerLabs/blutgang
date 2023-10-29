use crate::{
    admin::error::AdminError,
    Rpc,
};

use std::sync::{
    Arc,
    RwLock,
};

// use sled::Db;

pub fn execute_method(
    method: Option<&str>,
    params: Option<&str>,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    poverty_list: Arc<RwLock<Vec<Rpc>>>,
    // cache: Arc<Db>,
) -> Result<String, AdminError> {
    match method {
        Some("blutgang_quit") => admin_blutgang_quit(),
        Some("blutgang_rpc_list") => admin_list_rpc(rpc_list),
        Some("blutgang_poverty_list") => admin_list_rpc(poverty_list),
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
// We're returning a string and allowing unreachable code so rustc doesnt cry
#[allow(unreachable_code)]
fn admin_blutgang_quit() -> Result<String, AdminError> {
    std::process::exit(0);
    Ok("".to_string())
}

// List generic Fn to list RPCs from a Arc<RwLock<Vec<Rpc>>>
// Used for `blutgang_rpc_list` and `blutgang_poverty_list`
fn admin_list_rpc(rpc_list: Arc<RwLock<Vec<Rpc>>>) -> Result<String, AdminError> {
    let rpc_list = rpc_list.read().map_err(|_| AdminError::Innacessible)?;
    let mut rpc_list_str = String::new();
    for rpc in rpc_list.iter() {
        rpc_list_str.push_str(&format!("{:?}\n", rpc));
    }
    Ok(rpc_list_str)
}

// fn admin_db_stats(cache: Arc<Db>) -> Result<String, AdminError> {
//     let stats = cache.();
//     Ok(format!("{:?}", stats))
// }
