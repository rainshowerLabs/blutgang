// Errors
use crate::rpc::error::RpcError;
use std::error::Error;

#[derive(Debug)]
#[allow(dead_code)]
pub enum ConfigError {
    RpcError(String),
    BadConfig,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConfigError::RpcError(e) => write!(f, "Error while calling RPC: {}", e),
            ConfigError::BadConfig => write!(f, "Invalid Config File!"),
        }
    }
}

impl From<RpcError> for ConfigError {
    fn from(error: RpcError) -> Self {
        ConfigError::RpcError(error.to_string())
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ConfigError {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        ConfigError::RpcError(error.to_string())
    }
}

impl Error for ConfigError {}
