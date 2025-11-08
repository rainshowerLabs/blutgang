use crate::{
    admin::error::AdminError,
    database::types::{
        GenericBytes,
        RequestBus,
    },
    db_flush,
    Rpc,
    Settings,
};

use std::{
    fmt,
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

#[derive(Debug, thiserror::Error)]
#[error("failed to convert method to `BlutgangRpcMethod`:\n\ngot: {0:?}\nexpected:\n{1:#?}")]
pub struct Error<Method>(Method, &'static [&'static str])
where
    Method: fmt::Debug;
impl<Method> Error<Method>
where
    Method: fmt::Debug,
{
    pub fn new(method: Method) -> Self {
        Self(method, BlutgangRpcMethod::BLUTGANG_ALL)
    }
}

// @makemake -- This could make it easier to port code into a library for interacting with blutgang.
/// Available internal RPC method for Blutgang.
#[derive(Debug)]
pub enum BlutgangRpcMethod {
    Quit,
    RpcList,
    FlushCache,
    Config,
    PovertyList,
    Ttl,
    HealthCheckTtl,
    SetTtl,
    SetHealthCheckTtl,
    AddToRpcList,
    AddToPovertyList,
    RemoveFromRpcList,
    RemoveFromPovertyList,
}
impl BlutgangRpcMethod {
    const BLUTGANG_QUIT: &str = "blutgang_quit";
    const BLUTGANG_RPC_LIST: &str = "blutgang_rpc_list";
    const BLUTGANG_FLUSH_CACHE: &str = "blutgang_flush_cache";
    const BLUTGANG_CONFIG: &str = "blutgang_config";
    const BLUTGANG_POVERTY_LIST: &str = "blutgang_poverty_list";
    const BLUTGANG_TTL: &str = "blutgang_ttl";
    const BLUTGANG_HEALTH_CHECK_TTL: &str = "blutgang_health_check_ttl";
    const BLUTGANG_SET_TTL: &str = "blutgang_set_ttl";
    const BLUTGANG_SET_HEALTH_CHECK_TTL: &str = "blutgang_set_health_check_ttl";
    const BLUTGANG_ADD_TO_RPC_LIST: &str = "blutgang_add_to_rpc_list";
    const BLUTGANG_ADD_TO_POVERTY_LIST: &str = "blutgang_add_to_poverty_list";
    const BLUTGANG_REMOVE_FROM_RPC_LIST: &str = "blutgang_remove_from_rpc_list";
    const BLUTGANG_REMOVE_FROM_POVERTY_LIST: &str = "blutgang_remove_from_poverty_list";

    const BLUTGANG_ALL: &[&str; 13] = &[
        Self::BLUTGANG_QUIT,
        Self::BLUTGANG_RPC_LIST,
        Self::BLUTGANG_FLUSH_CACHE,
        Self::BLUTGANG_CONFIG,
        Self::BLUTGANG_POVERTY_LIST,
        Self::BLUTGANG_TTL,
        Self::BLUTGANG_HEALTH_CHECK_TTL,
        Self::BLUTGANG_SET_TTL,
        Self::BLUTGANG_SET_HEALTH_CHECK_TTL,
        Self::BLUTGANG_ADD_TO_RPC_LIST,
        Self::BLUTGANG_ADD_TO_POVERTY_LIST,
        Self::BLUTGANG_REMOVE_FROM_RPC_LIST,
        Self::BLUTGANG_REMOVE_FROM_POVERTY_LIST,
    ];

    /// Useful for circumventing lifetimes associated with `let` bindings.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Quit => Self::BLUTGANG_QUIT,
            Self::RpcList => Self::BLUTGANG_RPC_LIST,
            Self::FlushCache => Self::BLUTGANG_FLUSH_CACHE,
            Self::Config => Self::BLUTGANG_CONFIG,
            Self::PovertyList => Self::BLUTGANG_POVERTY_LIST,
            Self::Ttl => Self::BLUTGANG_TTL,
            Self::HealthCheckTtl => Self::BLUTGANG_HEALTH_CHECK_TTL,
            Self::SetTtl => Self::BLUTGANG_SET_TTL,
            Self::SetHealthCheckTtl => Self::BLUTGANG_SET_HEALTH_CHECK_TTL,
            Self::AddToRpcList => Self::BLUTGANG_ADD_TO_RPC_LIST,
            Self::AddToPovertyList => Self::BLUTGANG_ADD_TO_POVERTY_LIST,
            Self::RemoveFromRpcList => Self::BLUTGANG_REMOVE_FROM_RPC_LIST,
            Self::RemoveFromPovertyList => Self::BLUTGANG_REMOVE_FROM_POVERTY_LIST,
        }
    }
}
impl TryFrom<Option<&str>> for BlutgangRpcMethod {
    type Error = Error<Option<String>>;
    fn try_from(value: Option<&str>) -> Result<Self, Self::Error> {
        match value {
            Some(Self::BLUTGANG_QUIT) => Ok(Self::Quit),
            Some(Self::BLUTGANG_RPC_LIST) => Ok(Self::RpcList),
            Some(Self::BLUTGANG_FLUSH_CACHE) => Ok(Self::FlushCache),
            Some(Self::BLUTGANG_CONFIG) => Ok(Self::Config),
            Some(Self::BLUTGANG_POVERTY_LIST) => Ok(Self::PovertyList),
            Some(Self::BLUTGANG_TTL) => Ok(Self::Ttl),
            Some(Self::BLUTGANG_HEALTH_CHECK_TTL) => Ok(Self::HealthCheckTtl),
            Some(Self::BLUTGANG_SET_TTL) => Ok(Self::SetTtl),
            Some(Self::BLUTGANG_SET_HEALTH_CHECK_TTL) => Ok(Self::SetHealthCheckTtl),
            Some(Self::BLUTGANG_ADD_TO_RPC_LIST) => Ok(Self::AddToRpcList),
            Some(Self::BLUTGANG_ADD_TO_POVERTY_LIST) => Ok(Self::AddToPovertyList),
            Some(Self::BLUTGANG_REMOVE_FROM_RPC_LIST) => Ok(Self::RemoveFromRpcList),
            Some(Self::BLUTGANG_REMOVE_FROM_POVERTY_LIST) => Ok(Self::RemoveFromPovertyList),
            _ => Err(Error::new(value.map(ToString::to_string))),
        }
    }
}
impl serde::Serialize for BlutgangRpcMethod {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> serde::Deserialize<'de> for BlutgangRpcMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        match s {
            Self::BLUTGANG_QUIT => Ok(Self::Quit),
            Self::BLUTGANG_RPC_LIST => Ok(Self::RpcList),
            Self::BLUTGANG_FLUSH_CACHE => Ok(Self::FlushCache),
            Self::BLUTGANG_CONFIG => Ok(Self::Config),
            Self::BLUTGANG_POVERTY_LIST => Ok(Self::PovertyList),
            Self::BLUTGANG_TTL => Ok(Self::Ttl),
            Self::BLUTGANG_HEALTH_CHECK_TTL => Ok(Self::HealthCheckTtl),
            Self::BLUTGANG_SET_TTL => Ok(Self::SetTtl),
            Self::BLUTGANG_SET_HEALTH_CHECK_TTL => Ok(Self::SetHealthCheckTtl),
            Self::BLUTGANG_ADD_TO_RPC_LIST => Ok(Self::AddToRpcList),
            Self::BLUTGANG_ADD_TO_POVERTY_LIST => Ok(Self::AddToPovertyList),
            Self::BLUTGANG_REMOVE_FROM_RPC_LIST => Ok(Self::RemoveFromRpcList),
            Self::BLUTGANG_REMOVE_FROM_POVERTY_LIST => Ok(Self::RemoveFromPovertyList),
            _ => Err(serde::de::Error::unknown_variant(s, Self::BLUTGANG_ALL)),
        }
    }
}

/// Extract the method, call the appropriate function and return the response
pub async fn execute_method<K, V>(
    tx: Value,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    poverty_list: &Arc<RwLock<Vec<Rpc>>>,
    config: Arc<RwLock<Settings>>,
    cache: RequestBus<K, V>,
) -> Result<Value, AdminError>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let method = tx["method"].as_str().try_into();
    tracing::debug!("Method: {:?}", method);

    // Check if write protection is enabled
    let write_protection_enabled = config.read().unwrap().admin.readonly;

    match method {
        Ok(BlutgangRpcMethod::Quit) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_quit(cache).await
            }
        }
        Ok(BlutgangRpcMethod::RpcList) => admin_list_rpc(rpc_list),
        Ok(BlutgangRpcMethod::FlushCache) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_flush_cache(cache).await
            }
        }
        Ok(BlutgangRpcMethod::Config) => admin_config(config),
        Ok(BlutgangRpcMethod::PovertyList) => admin_list_rpc(poverty_list),
        Ok(BlutgangRpcMethod::Ttl) => admin_blutgang_ttl(config),
        Ok(BlutgangRpcMethod::HealthCheckTtl) => admin_blutgang_health_check_ttl(config),
        Ok(BlutgangRpcMethod::SetTtl) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_set_ttl(config, tx["params"].as_array())
            }
        }
        Ok(BlutgangRpcMethod::SetHealthCheckTtl) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_blutgang_set_health_check_ttl(config, tx["params"].as_array())
            }
        }
        Ok(BlutgangRpcMethod::AddToRpcList) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_add_rpc(rpc_list, tx["params"].as_array())
            }
        }
        Ok(BlutgangRpcMethod::AddToPovertyList) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_add_rpc(poverty_list, tx["params"].as_array())
            }
        }
        Ok(BlutgangRpcMethod::RemoveFromRpcList) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_remove_rpc(rpc_list, tx["params"].as_array())
            }
        }
        Ok(BlutgangRpcMethod::RemoveFromPovertyList) => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                admin_remove_rpc(poverty_list, tx["params"].as_array())
            }
        }
        Err(err) => Err(AdminError::InvalidMethod(err)),
    }
}

/// Quit Blutgang upon receiving this method
/// We're returning a Null and allowing unreachable code so rustc doesnt cry
#[allow(unreachable_code)]
async fn admin_blutgang_quit<K, V>(cache: RequestBus<K, V>) -> Result<Value, AdminError>
where
    K: GenericBytes,
    V: GenericBytes,
{
    // We're doing something not-good so flush everything to disk
    drop(db_flush!(cache));

    std::process::exit(0);
    Ok(Value::Null)
}

/// Flushes sled cache to disk
async fn admin_flush_cache<K, V>(cache: RequestBus<K, V>) -> Result<Value, AdminError>
where
    K: GenericBytes,
    V: GenericBytes,
{
    let time = Instant::now();
    drop(db_flush!(cache));
    let time = time.elapsed();

    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": format!("Cache flushed in {:?}", time),
    });

    Ok(rx)
}

/// Respond with the config we started blutgang with
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

/// List generic Fn to retrieve RPCs from an Arc<RwLock<Vec<Rpc>>>
/// Used for `blutgang_rpc_list` and `blutgang_poverty_list`
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

/// Pushes an RPC to the end of the list:
/// - param[0] - RPC url
/// - param[1] - max_consecutive
/// - param[2] - ma_len
/// - param[3] - ma_len
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
        rpc.parse().unwrap(),
        ws_url.map(|ws_url| ws_url.parse().unwrap()),
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

/// Remove RPC at a specified index, return the url of the removed RPC:
/// - param[0] - RPC index
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

/// Responds with health_check_ttl
fn admin_blutgang_health_check_ttl(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.health_check_ttl,
    });

    Ok(rx)
}

/// Responds with ttl
fn admin_blutgang_ttl(config: Arc<RwLock<Settings>>) -> Result<Value, AdminError> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": guard.ttl,
    });

    Ok(rx)
}

/// Sets health_check_ttl:
/// - param[0] - health_check_ttl
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

/// Sets ttl:
/// param[0] - ttl
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
    use crate::database_processing;
    use jsonwebtoken::DecodingKey;
    use sled::Config;
    use sled::Db;
    use tokio::sync::mpsc;

    // Helper function to create a test RPC list
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".parse().unwrap(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    // Helper function to create a test poverty list
    fn create_test_poverty_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://poverty.com".parse().unwrap(),
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
    fn create_test_cache() -> RequestBus<Vec<u8>, Vec<u8>> {
        let cache = Config::tmp().unwrap();
        let cache = Db::open_with_config(&cache).unwrap();
        let (db_tx, db_rx) = mpsc::unbounded_channel();
        tokio::task::spawn(database_processing(db_rx, cache));

        db_tx
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_execute_method_blutgang_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::RpcList });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_flush_cache() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::FlushCache });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_config() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::Config });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_poverty_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::PovertyList });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::Ttl });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_health_check_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::HealthCheckTtl });

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
    #[serial_test::serial]
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
    #[serial_test::serial]
    async fn test_execute_method_add_to_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::AddToRpcList, "params": ["http://example.com", "ws://example.com", 5, 10, 0.5] });

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
    #[serial_test::serial]
    async fn test_execute_method_add_to_rpc_list_no_ws() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::AddToRpcList, "params": ["http://example.com", Null, 5, 10, 0.5] });

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
    #[serial_test::serial]
    async fn test_execute_method_remove_from_rpc_list() {
        // Arrange
        let cache = create_test_cache();
        // purpusefully OOB
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::RemoveFromRpcList, "params": [10] });

        let rpc_list = create_test_rpc_list();
        // rpc_list has only 1 so add another one to keep the 1st one some company
        rpc_list.write().unwrap().push(Rpc::new(
            "http://example.com".parse().unwrap(),
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
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::RemoveFromRpcList, "params": [0] });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_set_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::SetTtl, "params": [9001] });

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
    #[serial_test::serial]
    async fn test_execute_method_blutgang_set_health_check_ttl() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::SetHealthCheckTtl, "params": [9001] });

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
    #[serial_test::serial]
    async fn test_rw_protection() {
        // Arrange
        let cache = create_test_cache();
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::SetHealthCheckTtl, "params": [9001] });

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
        let tx = json!({ "id":1,"method": BlutgangRpcMethod::HealthCheckTtl });
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
