//! Configuration errors

use std::{
    io,
    path,
};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    RpcError(#[from] crate::rpc::error::RpcError),

    #[error("Node is syncing!")]
    Syncing,

    #[error("failed to read config file '{}': {err:?}", config.display())]
    ReadError {
        config: path::PathBuf,
        err: io::Error,
    },

    #[error("failed to deserialize config file '{}': {err:?}", config.display())]
    FailedDeserialization {
        config: path::PathBuf,
        err: toml::de::Error,
    },
}
