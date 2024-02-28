use crate::{
    admin::error::AdminError,
    Rpc,
    Settings,
};

use std::{
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

// Extract the method, call the appropriate function and return the response
pub async fn execute_method(
    tx: Value,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    config: Arc<RwLock<Settings>>,
    cache: Arc<Db>,
) -> Result<Value, AdminError> {
    let method = tx["method"].as_str();
    println!("Method: {:?}", method.unwrap_or("None"));

    // Check if write protection is enabled
    let write_protection_enabled = config.read().unwrap().admin.readonly;

    match method {
        Some("blutgang_quit") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_quit(cache).await
            }
        }
        Some("blutgang_rpc_list") => admin_list_rpc(rpc_list),
        Some("blutgang_flush_cache") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_flush_cache(cache).await
            }
        }
        Some("blutgang_config") => admin_config(config),
        Some("blutgang_poverty_list") => admin_list_rpc(poverty_list),
        Some("blutgang_ttl") => admin_blutgang_ttl(config),
        Some("blutgang_health_check_ttl") => admin_blutgang_health_check_ttl(config),
        Some("blutgang_set_ttl") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_set_ttl(config, tx["params"].as_array())
            }
        }
        Some("blutgang_set_health_check_ttl") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_set_health_check_ttl(config, tx["params"].as_array())
            }
        }
        Some("blutgang_add_to_rpc_list") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_add_rpc(rpc_list, tx["params"].as_array())
            }
        }
        Some("blutgang_add_to_poverty_list") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_add_rpc(poverty_list, tx["params"].as_array())
            }
        }
        Some("blutgang_remove_from_rpc_list") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_remove_rpc(rpc_list, tx["params"].as_array())
            }
        }
        Some("blutgang_remove_from_poverty_list") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_remove_rpc(poverty_list, tx["params"].as_array())
            }
        }
        Some(_) => Err(AdminError::InvalidMethod),
        _ => Ok(().into()),
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
    // unsafe {
    //     ptr::drop_in_place(Arc::into_raw(cache) as *mut Db);
    // }
    std::process::exit(0);
    Ok(Value::Null)
}

// Flushes sled cache to disk
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

// Respond with the config we started blutgang with
fn admin_config(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": {
            "address": guard.address,
            "do_clear": guard.do_clear,
            "health_check": guard.health_check,
            "admin": {
                "enabled": guard.admin.enabled,
                "readonly": guard.admin.readonly,
            },
            "ttl": guard.ttl,
            "health_check_ttl": guard.health_check_ttl,
        },
    });

    Ok(rx)
}

// List generic Fn to retrieve RPCs from an Arc<RwLock<Vec<Rpc>>>
// Used for `blutgang_rpc_list` and `blutgang_poverty_list`
fn admin_list_rpc(rpc_list: &Arc<RwLock<Vec<Rpc>>>) -> Result<Value, AdminError> {
    // Read the RPC list, handling errors
    let rpc_list = rpc_list.read().map_err(|_| AdminError::Inaccessible)?;

    // Prepare a formatted string for the RPC list
    let mut rpc_list_str = String::new();
    rpc_list_str.push('[');

    // Iterate over the RPC list and format each RPC
    for rpc in rpc_list.iter() {
        rpc_list_str.push_str(&format!(
            "{{\"name\": \"{}\", \"max_consecutive\": {}, \"last_error\": {}}}",
            rpc.name, rpc.max_consecutive, rpc.status.last_error
        ));
    }

    // Complete the formatted RPC list string
    rpc_list_str.push(']');

    // Create a JSON response
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
// param[3] - ma_len
fn admin_add_rpc(
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    params: Option<&Vec<Value>>,
) -> Result<Value, AdminError> {
    let params = match params {
        Some(params) => params,
        None => return Err(AdminError::InvalidParams),
    };

    if params.len() != 5 {
        return Err(AdminError::InvalidLen);
    }

    let rpc = match params[0].as_str() {
        Some(rpc) => rpc,
        None => return Err(AdminError::ParseError),
    };

    // ws_url is optional so it can be none
    let ws_url = match params[1].is_null() {
        true => None,
        false => {
            match params[1].as_str().map(|s| s.to_string()) {
                Some(ws_url) => Some(ws_url),
                None => return Err(AdminError::ParseError),
            }
        }
    };

    let max_consecutive = params[2]
        .to_string()
        .replace('\"', "")
        .parse::<u32>()
        .unwrap_or(0);
    let mut delta = params[3]
        .to_string()
        .replace('\"', "")
        .parse::<u64>()
        .unwrap_or(0);
    let ma_len = params[4]
        .to_string()
        .replace('\"', "")
        .parse::<f64>()
        .unwrap_or(0.0);

    if delta != 0 {
        delta = 1_000_000 / delta;
    }

    let mut rpc_list = rpc_list.write().map_err(|_| AdminError::Inaccessible)?;

    rpc_list.push(Rpc::new(
        rpc.to_string(),
        ws_url,
        max_consecutive,
        delta.into(),
        ma_len,
    ));

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": format!("RPC: {}, max_consecutive: {}, ma: {}", rpc, max_consecutive, ma_len),
    });

    Ok(rx)
}

// Remove RPC at a specified index, return the url of the removed RPC
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
        return Err(AdminError::InvalidLen);
    }

    let index = match params[0].to_string().replace('\"', "").parse::<u64>() {
        Ok(index) => index,
        Err(_) => return Err(AdminError::ParseError),
    };

    let mut rpc_list = rpc_list.write().map_err(|_| AdminError::Inaccessible)?;

    // Check if index exists before removing
    if index as usize >= rpc_list.len() {
        return Err(AdminError::OutOfBounds);
    }

    // Finally, remove the index
    let removed: Rpc = rpc_list.remove(index as usize);

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": removed.name,
    });

    Ok(rx)
}

// TODO: change the following 4 fn so theyre generic

// Responds with health_check_ttl
fn admin_blutgang_health_check_ttl(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.health_check_ttl,
    });

    Ok(rx)
}

// Responds with ttl
fn admin_blutgang_ttl(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.ttl,
    });

    Ok(rx)
}

// Sets health_check_ttl
//
// param[0] - health_check_ttl
fn admin_blutgang_set_health_check_ttl(
    config: Arc<RwLock<Settings>>,
    params: Option<&Vec<Value>>,
) -> Result<Value, AdminError> {
    let params = match params {
        Some(params) => params,
        None => return Err(AdminError::InvalidParams),
    };

    if params.len() != 1 {
        return Err(AdminError::InvalidLen);
    }

    let health_check_ttl = match params[0].to_string().replace('\"', "").parse::<u64>() {
        Ok(health_check_ttl) => health_check_ttl,
        Err(_) => return Err(AdminError::ParseError),
    };

    let mut guard = config.write().unwrap();
    guard.health_check_ttl = health_check_ttl;

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.health_check_ttl,
    });

    Ok(rx)
}

// Sets ttl
//
// param[0] - ttl
fn admin_blutgang_set_ttl(
    config: Arc<RwLock<Settings>>,
    params: Option<&Vec<Value>>,
) -> Result<Value, AdminError> {
    let params = match params {
        Some(params) => params,
        None => return Err(AdminError::InvalidParams),
    };

    if params.len() != 1 {
        return Err(AdminError::InvalidLen);
    }

    let ttl = match params[0].to_string().replace('\"', "").parse::<u64>() {
        Ok(ttl) => ttl,
        Err(_) => return Err(AdminError::ParseError),
    };

    let mut guard = config.write().unwrap();
    guard.ttl = ttl as u128;

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.ttl,
    });

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::DecodingKey;

    // Helper function to create a test RPC list
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    // Helper function to create a test poverty list
    fn create_test_poverty_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://poverty.com".to_string(),
            None,
            2,
            1000,
            0.1,
        )]))
    }

    // Helper function to create a test Settings config
    fn create_test_settings_config() -> Arc<RwLock<Settings>> {
        let mut config = Settings::default();
        config.do_clear = true;
        config.admin.key = DecodingKey::from_secret(b"some-key");
        Arc::new(RwLock::new(config))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> Arc<Db> {
        let db = sled::Config::new().temporary(true);
        let db = db.open().unwrap();

        Arc::new(db)
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_rpc_list" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_flush_cache() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_flush_cache" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok()); // Verify that flushing the cache doesn't produce an error
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_config() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_config" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_poverty_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_poverty_list" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_ttl" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_health_check_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_health_check_ttl" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_method_invalid_method() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "invalid_method" });

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_method_add_to_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_add_to_rpc_list", "params": ["http://example.com", "ws://example.com", 5, 10, 0.5] });

        let rpc_list = create_test_rpc_list();
        let len = rpc_list.read().unwrap().len();

        // Act
        let result = execute_method(
            tx,
            &rpc_list,
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
        assert!(rpc_list.read().unwrap().len() == len + 1);
    }

    #[tokio::test]
    async fn test_execute_method_add_to_rpc_list_no_ws() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_add_to_rpc_list", "params": ["http://example.com", Null, 5, 10, 0.5] });

        let rpc_list = create_test_rpc_list();
        let len = rpc_list.read().unwrap().len();

        // Act
        let result = execute_method(
            tx,
            &rpc_list,
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
        assert!(rpc_list.read().unwrap().len() == len + 1);
    }

    #[tokio::test]
    async fn test_execute_method_remove_from_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        // purpusefully OOB
        let tx = json!({ "id":1,"method": "blutgang_remove_from_rpc_list", "params": [10] });

        let rpc_list = create_test_rpc_list();
        // rpc_list has only 1 so add another one to keep the 1st one some company
        rpc_list.write().unwrap().push(Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        ));
        let len = rpc_list.read().unwrap().len();

        // Act
        let result = execute_method(
            tx,
            &rpc_list,
            &create_test_poverty_list(),
            create_test_settings_config(),
            cache.clone(),
        )
        .await;

        // Assert
        assert!(result.is_err());

        // Arrange
        let tx = json!({ "id":1,"method": "blutgang_remove_from_rpc_list", "params": [0] });

        // Act
        let binding = create_test_poverty_list();
        let result = execute_method(
            tx,
            &rpc_list,
            &binding,
            create_test_settings_config(),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
        assert!(rpc_list.read().unwrap().len() == len - 1);
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_set_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_set_ttl", "params": [9001] });

        let config = create_test_settings_config();
        let ttl = config.read().unwrap().ttl;

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            Arc::clone(&config),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
        assert!(config.read().unwrap().ttl != ttl);
        assert!(config.read().unwrap().ttl == 9001)
    }

    #[tokio::test]
    async fn test_execute_method_blutgang_set_health_check_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_set_health_check_ttl", "params": [9001] });

        let config = create_test_settings_config();
        let health_check_ttl = config.read().unwrap().health_check_ttl;

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            Arc::clone(&config),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
        assert!(config.read().unwrap().health_check_ttl != health_check_ttl);
        assert!(config.read().unwrap().health_check_ttl == 9001)
    }

    #[tokio::test]
    async fn test_rw_protection() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": "blutgang_set_health_check_ttl", "params": [9001] });

        let config = create_test_settings_config();
        config.write().unwrap().admin.readonly = true;

        // Act
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            Arc::clone(&config),
            cache.clone(),
        )
        .await;

        // Assert
        assert!(result.is_err());

        // Also check that we can read
        let tx = json!({ "id":1,"method": "blutgang_health_check_ttl" });
        let result = execute_method(
            tx,
            &create_test_rpc_list(),
            &create_test_poverty_list(),
            Arc::clone(&config),
            cache,
        )
        .await;

        // Assert
        assert!(result.is_ok());
    }
}
